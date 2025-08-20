use anyhow::anyhow;
use arc_swap::ArcSwap;
use futures::future::{self};
use konf_provider::loader::{JsonLoader, MultiLoader, MultiWriter, Value, YamlLoader};
use konf_provider::render::resolve_refs_from_deps;
use konf_provider::utils::MyError;
use konf_provider::{DagFiles, Konf, utils};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use xitca_web::handler::params::Params;
use xitca_web::middleware::tower_http_compat::TowerHttpCompat;
use xitca_web::{
    App,
    handler::{handler_service, state::StateRef},
    route::get,
};

#[derive(Debug)]
struct AppState {
    dag: Dag,
    writer: Arc<MultiWriter>,
}

fn main() -> std::io::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                format!(
                    "{}=debug,tower_http=debug,axum::rejection=trace",
                    env!("CARGO_CRATE_NAME")
                )
                .into()
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
    let folder = "example".parse::<PathBuf>().unwrap();
    let multiloader = MultiLoader::new(vec![Box::new(YamlLoader {})]);
    let d = Dag::new(folder, multiloader).expect("failed to read directory");

    let multiwriter = MultiWriter::new(vec![Box::new(YamlLoader {}), Box::new(JsonLoader {})]);

    let state = Arc::from(AppState {
        dag: d,
        writer: Arc::from(multiwriter),
    });

    App::new()
        .with_state(state)
        .at("/live", get(handler_service(async || "OK")))
        .at("/reload", get(handler_service(reload)))
        .at("/data/:format", get(handler_service(get_data)))
        .enclosed_fn(utils::error_handler)
        .enclosed(TowerHttpCompat::new(TraceLayer::new_for_http()))
        .serve()
        .bind(format!("0.0.0.0:{}", &4000))?
        .run()
        .wait()
}

async fn get_data(
    Params(format): Params<String>,
    StateRef(state): StateRef<'_, AppState>,
) -> Result<String, MyError> {
    // load form commit <hash>:hash(content + filename)
    let d = state
        .dag
        .get_rendered("c")
        .await
        .map_err(|_| MyError(anyhow::Error::msg("missing item")))?;

    state
        .writer
        .write(&format, &d)
        .ok_or(MyError(anyhow::Error::msg("Failed to get format")))
}

async fn reload(StateRef(state): StateRef<'_, AppState>) -> Result<String, MyError> {
    state
        .dag
        .reload()
        .map_err(|_| MyError(anyhow::Error::msg("failed to reload conf")))?;
    Ok("OK".to_string())
}

// The state that will be swapped atomically. It must be cheap to clone the Arc, not the data.

#[derive(Debug)]
pub struct DagInner {
    // Stable configuration
    folder: PathBuf,
    multiloader: Arc<MultiLoader>,

    // The dynamically reloaded, shared state
    files: ArcSwap<DagFiles>,
}

#[derive(Clone, Debug)]
pub struct Dag {
    inner: Arc<DagInner>,
}

#[derive(Debug, Clone)]
pub enum RenderError {
    All,
}

#[derive(Debug, Clone)]
pub enum MyResult {
    Ok(Value),
    Err(RenderError),
}

impl Dag {
    // new() now initializes the ArcSwap with the first load
    pub fn new(folder: PathBuf, multiloader: MultiLoader) -> anyhow::Result<Self> {
        let inner = Arc::new(DagInner {
            folder,
            multiloader: Arc::from(multiloader),
            files: ArcSwap::default(), // Start with an empty HashMap
        });
        let handle = Self { inner };
        handle.reload()?; // Call reload on the new handle
        Ok(handle)
    }
    pub async fn get_rendered(&self, file_path: &str) -> anyhow::Result<Value> {
        let files_snapshot = self.inner.files.load();
        let konf = files_snapshot
            .get(file_path)
            .ok_or_else(|| anyhow!("File not found: {}", file_path))?;

        // This `get_or_try_init` takes a Future, and the whole expression is await-able.
        // This now correctly matches what the compiler expects.
        let rendered_value = konf
            .rendered
            .get_or_try_init(async {
                // The async block is now valid
                let raw_value = konf.raw.clone();
                let imports = get_imports(&raw_value);

                let dep_futures = imports.iter().map(|key| self.get_rendered(key));

                let dep_results = future::try_join_all(dep_futures).await?;
                let deps_map = imports.into_iter().zip(dep_results).collect();

                let mut value_to_render = raw_value;
                resolve_refs_from_deps(&mut value_to_render, &deps_map);

                // The future must resolve to a Result<Value, E>
                Ok::<_, anyhow::Error>(value_to_render)
            })
            .await?; // We await the final result here.

        Ok(rendered_value.clone())
    }

    // reload() now takes &self and updates the ArcSwap
    pub fn reload(&self) -> anyhow::Result<()> {
        let paths = fs::read_dir(&self.inner.folder)?;
        let mut files: DagFiles = HashMap::new();
        for path in paths {
            let p1 = path?.file_name();
            let p = p1.to_str().ok_or(anyhow!("failed to read filename"))?;
            let mut m_folder = self.inner.folder.clone();
            m_folder.push(p);
            let content = fs::read_to_string(m_folder)?;
            let k = Konf::new(self.inner.multiloader.load(p, &content)?);

            let path_no_ext = p.split('.').next().ok_or(anyhow!("invalid path"))?;
            files.insert(path_no_ext.to_string(), k);
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

fn get_imports(value: &Value) -> Vec<String> {
    const IMPORT_KEY: &str = "import";
    value
        .get(IMPORT_KEY)
        .and_then(|e| e.as_sequence())
        .map(|seq| {
            seq.iter()
                .filter_map(|e| e.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}
