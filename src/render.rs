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

#[derive(Debug, Clone)]
pub enum RenderError {
    All,
}

#[derive(Debug)]
pub struct DagInner<P: FileProvider> {
    // Stable configuration
    file_provider: P,
    multiloader: Arc<MultiLoader>,

    // The dynamically reloaded, shared state
    files: ArcSwap<DagFiles>,
}

#[derive(Clone, Debug)]
pub struct Dag<P: FileProvider> {
    inner: Arc<DagInner<P>>,
}

impl<P: FileProvider> Dag<P> {
    // new() now initializes the ArcSwap with the first load
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

    // reload() now takes &self and updates the ArcSwap
    pub async fn reload(&self) -> Result<(), LoaderError> {
        let paths = self.inner.file_provider.list().await;
        let mut files: DagFiles = HashMap::new();

        for path in paths {
            if let Some(content) = self.inner.file_provider.load(&path.full_path).await {
                let k = Konf::new(self.inner.multiloader.load(&path.ext, &content)?);
                files.insert(path.filename, k);
            }
        }
        // Atomically publish the new HashMap
        self.inner.files.store(Arc::new(files));
        Ok(())
    }

    pub fn get_raw(&self, file_path: &str) -> Result<Value, RenderError> {
        // Load the current snapshot of files to perform the read
        let files_snapshot = self.inner.files.load();
        files_snapshot
            .get(file_path)
            .map(|v| v.raw.clone())
            .ok_or(RenderError::All)
    }
}
