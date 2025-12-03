use anyhow::anyhow;
use std::{collections::HashMap, sync::Arc};

use arc_swap::ArcSwap;
use futures::future;

use crate::{
    DagFiles, Konf, Value,
    fs::FileProvider,
    loader::{LoaderError, MultiLoader},
    render_helper::resolve_refs_from_deps,
    utils::get_conf_strings,
};

/// Error type for configuration rendering failures.
#[derive(Debug, Clone)]
pub enum RenderError {
    /// Generic render error.
    All,
}

#[derive(Debug)]
struct DagInner<P: FileProvider> {
    /// The file provider used to load configuration files.
    file_provider: P,
    /// Multi-format loader for parsing configuration files.
    multiloader: Arc<MultiLoader>,
    /// Atomically swappable map of loaded configuration files.
    files: ArcSwap<DagFiles>,
}

/// A directed acyclic graph of configuration files with dependency resolution.
///
/// The DAG loads configuration files from a `FileProvider`, parses them using
/// a `MultiLoader`, and resolves template references between files. It supports
/// atomic hot-reloading of configurations.
///
/// # Template Syntax
///
/// Configuration files can reference values from other files using the `${path}` syntax:
/// - `${other_file.key}` - References `key` from `other_file`
/// - `${other_file.nested.key}` - References nested values
///
/// Files declare their dependencies in a special `<!>` metadata section:
/// ```yaml
/// <!>:
///   import:
///     - base_config
///     - secrets
/// ```
#[derive(Clone, Debug)]
pub struct Dag<P: FileProvider> {
    inner: Arc<DagInner<P>>,
}

impl<P: FileProvider> Dag<P> {
    /// Creates a new DAG and loads all configuration files.
    ///
    /// This will read all files from the provider, parse them, and prepare
    /// them for rendering. The initial load happens synchronously.
    pub async fn new(file_provider: P, multiloader: Arc<MultiLoader>) -> anyhow::Result<Self> {
        let inner = Arc::new(DagInner {
            file_provider,
            multiloader,
            files: ArcSwap::default(), // Start with an empty HashMap
        });
        let handle = Self { inner };
        handle.reload().await?;
        Ok(handle)
    }
    /// Returns the fully rendered configuration for the given file path.
    ///
    /// The rendering is lazy and cached - the first call computes the result,
    /// subsequent calls return the cached value. Template variables are resolved
    /// by recursively rendering imported files.
    pub async fn get_rendered(&self, file_path: &str) -> anyhow::Result<Value> {
        let files_snapshot = self.inner.files.load();
        let konf = files_snapshot
            .get(file_path)
            .ok_or_else(|| anyhow!("File not found: {}", file_path))?;
        const IMPORT_KEY: &str = "import";
        // This `get_or_try_init` takes a Future, and the whole expression is await-able.
        // This now correctly matches what the compiler expects.
        let rendered_value = konf
            .rendered
            .get_or_try_init(async {
                // The async block is now valid
                let raw_value = konf.raw.clone();
                let imports = get_conf_strings(&raw_value, IMPORT_KEY);

                let dep_futures = imports.iter().map(|key| self.get_rendered(key));

                let dep_results = future::try_join_all(dep_futures).await?;
                let deps_map = imports.into_iter().zip(dep_results).collect();

                let mut value_to_render = raw_value;
                resolve_refs_from_deps(&mut value_to_render, &deps_map);

                if let Value::Mapping(ref mut m) = value_to_render {
                    m.remove("<!>");
                };

                // The future must resolve to a Result<Value, E>
                Ok::<_, anyhow::Error>(value_to_render)
            })
            .await?; // We await the final result here.

        Ok(rendered_value.clone())
    }

    /// Reloads all configuration files from the provider.
    ///
    /// This atomically replaces all loaded configurations. Any cached
    /// rendered values are invalidated and will be recomputed on next access.
    pub async fn reload(&self) -> Result<(), LoaderError> {
        let paths = self.inner.file_provider.list().await;
        let mut files: DagFiles = HashMap::new();

        for path in paths {
            if let Some(content) = self.inner.file_provider.load(&path.full_path).await {
                match self.inner.multiloader.load(&path.ext, &content) {
                    Ok(l) => {
                        let k = Konf::new(l);
                        files.insert(path.filename, k);
                    }
                    Err(_) => {
                        tracing::warn!("failed to load {:?}", &path)
                    }
                }
            }
        }
        // Atomically publish the new HashMap
        self.inner.files.store(Arc::new(files));
        Ok(())
    }

    /// Returns the raw (unrendered) configuration value for the given file.
    pub fn get_raw(&self, file_path: &str) -> Result<Value, RenderError> {
        let files_snapshot = self.inner.files.load();
        files_snapshot
            .get(file_path)
            .map(|v| v.raw.clone())
            .ok_or(RenderError::All)
    }
}
