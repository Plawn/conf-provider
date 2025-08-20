use anyhow::anyhow;
use arc_swap::ArcSwap;
use dashmap::{DashMap, Entry};
use futures::FutureExt;
use futures::future::{self};
use konf_provider::loader::{JsonLoader, MultiLoader, MultiWriter, Value, YamlLoader};
use konf_provider::render::resolve_refs_from_deps;
use konf_provider::utils::MyError;
use konf_provider::{DagFiles, InFlightFuture, Konf, SharedResult, utils};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use xitca_web::handler::params::Params;
use xitca_web::middleware::tower_http_compat::TowerHttpCompat;
use xitca_web::{
    App,
    handler::{handler_service, json::Json, state::StateRef},
    route::{get, post},
};

#[derive(Debug)]
struct AppState {
    dag: Dag,
    writer: Arc<MultiWriter>,
}

fn main() -> std::io::Result<()> {
    // tracing_subscriber::registry()
    //     .with(
    //         tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
    //             format!(
    //                 "{}=debug,tower_http=debug,axum::rejection=trace",
    //                 env!("CARGO_CRATE_NAME")
    //             )
    //             .into()
    //         }),
    //     )
    //     .with(tracing_subscriber::fmt::layer())
    //     .init();
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
        // .enclosed(TowerHttpCompat::new(TraceLayer::new_for_http()))
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

    in_flight_renders: DashMap<String, InFlightFuture>,
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
            in_flight_renders: DashMap::new(),
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

                let dep_futures: Vec<_> =
                    imports.iter().map(|key| self.get_rendered(key)).collect();

                let dep_results = future::try_join_all(dep_futures).await?;
                let deps_map: HashMap<String, Value> =
                    imports.into_iter().zip(dep_results).collect();

                let mut value_to_render = raw_value;
                resolve_refs_from_deps(&mut value_to_render, &deps_map);

                // The future must resolve to a Result<Value, E>
                Ok::<_, anyhow::Error>(value_to_render)
            })
            .await?; // We await the final result here.

        Ok(rendered_value.clone())
    }

    // fn render_or_get_in_flight(&self, file_path: &str) -> InFlightFuture {
    //     // First, do a quick check to see if a future is already in flight.
    //     if let Some(entry) = self.inner.in_flight_renders.get(file_path) {
    //         return entry.value().clone();
    //     }

    //     // If not, we'll create a new future for rendering.
    //     let self_clone = self.clone();
    //     let path_owned = file_path.to_string();

    //     let render_future = async move {
    //         let result: anyhow::Result<Value> = async {
    //             let global_snapshot = self_clone.inner.files.load();
    //             let raw_value = global_snapshot
    //                 .get(&path_owned)
    //                 .ok_or_else(|| anyhow!("File not found: {}", path_owned))?
    //                 .raw
    //                 .clone();
    //             let imports = get_imports(&raw_value);

    //             let dep_futures: Vec<_> = imports
    //                 .iter()
    //                 .map(|key| self_clone.get_rendered(key))
    //                 .collect();

    //             let dep_results = future::try_join_all(dep_futures).await?;
    //             let deps_map: HashMap<String, Value> =
    //                 imports.into_iter().zip(dep_results).collect();

    //             let mut value_to_render = raw_value;
    //             resolve_refs_from_deps(&mut value_to_render, &deps_map);

    //             self_clone
    //                 .commit_single_result(&path_owned, value_to_render.clone())
    //                 .await;
    //             Ok(value_to_render)
    //         }
    //         .await;

    //         // Convert the final result into the shared format.
    //         result.map_err(Arc::new)
    //     };

    //     // This is where we explicitly assert that our future is `Send`.
    //     let boxed_future: Pin<Box<dyn futures::Future<Output = SharedResult> + Send + Sync>> =
    //         Box::pin(render_future);
    //     let shared_future = boxed_future.shared();

    //     // Now, use the entry API to atomically insert or get the existing future.
    //     // This handles the race condition where two threads try to render the same file.
    //     match self.inner.in_flight_renders.entry(file_path.to_string()) {
    //         Entry::Occupied(entry) => {
    //             // Another thread won the race. Use its future.
    //             entry.get().clone()
    //         }
    //         Entry::Vacant(entry) => {
    //             // We won the race. Insert our future and return it.
    //             entry.insert(shared_future.clone());
    //             shared_future
    //         }
    //     }
    // }

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
            let k = Konf::new(self.inner.multiloader.load(&p, &content)?);

            let path_no_ext = p.split('.').next().ok_or(anyhow!("invalid path"))?;
            files.insert(path_no_ext.to_string(), k);
        }

        // Atomically publish the new HashMap
        self.inner.files.store(Arc::new(files));
        self.inner.in_flight_renders.clear();
        Ok(())
    }

    pub fn get_raw(&self, file_path: &str) -> Result<Value, RenderError> {
        // Load the current snapshot of files to perform the read
        let files_snapshot = self.inner.files.load();
        files_snapshot
            .get(file_path)
            .map(|v| v.raw.clone())
            .ok_or_else(|| RenderError::All)
    }

    // New helper to commit just one file's result
    // async fn commit_single_result(&self, key: &str, value: Value) {
    //     loop {
    //         let guard = self.inner.files.load();
    //         let mut new_map = (**guard).clone();
    //         if let Some(k) = new_map.get_mut(key) {
    //             k.rendered = Some(value.clone());
    //             let new_arc = Arc::new(new_map);
    //             if Arc::ptr_eq(&guard, &self.inner.files.compare_and_swap(&guard, new_arc)) {
    //                 return;
    //             }
    //         } else {
    //             // File was deleted during render, just abort the commit.
    //             return;
    //         }
    //     }
    // }
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
