use anyhow::anyhow;
use arc_swap::ArcSwap;
use dashmap::{DashMap, Entry};
use futures::FutureExt;
use futures::future::{self, Shared};
use konf_provider::loader::{JsonLoader, MultiLoader, MultiWriter, Value, YamlLoader};
use konf_provider::render::resolve_refs_from_deps;
use konf_provider::utils::MyError;
use konf_provider::{DagFiles, InFlightFuture, Konf, RenderCache, SharedResult, utils};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::{fmt, fs};
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

// Manual Debug implementation for the handle
impl fmt::Debug for Dag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Dag")
            .field("folder", &self.inner.folder)
            .field("multiloader", &self.inner.multiloader)
            .field("files", &self.inner.files.load())
            .field(
                "in_flight_renders_count",
                &self.inner.in_flight_renders.len(),
            )
            .finish()
    }
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
    println!("d: {:?}", &d);
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

    in_flight_renders: DashMap<String, InFlightFuture>,
}

#[derive(Clone)]
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
    /// This is the refactored, public-facing async function.
    pub async fn get_rendered(&self, file_path: &str) -> anyhow::Result<Value> {
        // 1. Fast path: Check persistent cache (no locking needed)
        if let Some(k) = self.inner.files.load().get(file_path) {
            if let Some(r) = &k.rendered {
                return Ok(r.clone());
            }
        }

        // 2. Slow path: Coalesce work using a helper function.
        // The helper returns a future that is guaranteed to be Send.
        let future_to_await = self.render_or_get_in_flight(file_path);

        // 3. Await the result.
        let shared_result = future_to_await.await;

        // 4. Clean up the in-flight map for this specific request.
        self.inner.in_flight_renders.remove(file_path);

        // 5. Convert the internal SharedResult back to the public anyhow::Result.
        shared_result.map_err(|arc_err| anyhow!(arc_err.to_string()))
    }
    fn render_or_get_in_flight(&self, file_path: &str) -> InFlightFuture {
        // First, do a quick check to see if a future is already in flight.
        if let Some(entry) = self.inner.in_flight_renders.get(file_path) {
            return entry.value().clone();
        }

        // If not, we'll create a new future for rendering.
        let self_clone = self.clone();
        let path_owned = file_path.to_string();

        let render_future = async move {
            let result: anyhow::Result<Value> = async {
                let global_snapshot = self_clone.inner.files.load();
                let raw_value = global_snapshot
                    .get(&path_owned)
                    .ok_or_else(|| anyhow!("File not found: {}", path_owned))?
                    .raw
                    .clone();
                let imports = get_imports(&raw_value);

                let dep_futures: Vec<_> = imports
                    .iter()
                    .map(|key| self_clone.get_rendered(key))
                    .collect();

                let dep_results = future::try_join_all(dep_futures).await?;
                let deps_map: HashMap<String, Value> =
                    imports.into_iter().zip(dep_results).collect();

                let mut value_to_render = raw_value;
                resolve_refs_from_deps(&mut value_to_render, &deps_map);

                self_clone
                    .commit_single_result(&path_owned, value_to_render.clone())
                    .await;
                Ok(value_to_render)
            }
            .await;

            // Convert the final result into the shared format.
            result.map_err(Arc::new)
        };

        // This is where we explicitly assert that our future is `Send`.
        let boxed_future: Pin<Box<dyn futures::Future<Output = SharedResult> + Send + Sync>> =
            Box::pin(render_future);
        let shared_future = boxed_future.shared();

        // Now, use the entry API to atomically insert or get the existing future.
        // This handles the race condition where two threads try to render the same file.
        match self.inner.in_flight_renders.entry(file_path.to_string()) {
            Entry::Occupied(entry) => {
                // Another thread won the race. Use its future.
                entry.get().clone()
            }
            Entry::Vacant(entry) => {
                // We won the race. Insert our future and return it.
                entry.insert(shared_future.clone());
                shared_future
            }
        }
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
    async fn commit_single_result(&self, key: &str, value: Value) {
        loop {
            let guard = self.inner.files.load();
            let mut new_map = (**guard).clone();
            if let Some(k) = new_map.get_mut(key) {
                k.rendered = Some(value.clone());
                let new_arc = Arc::new(new_map);
                if Arc::ptr_eq(&guard, &self.inner.files.compare_and_swap(&guard, new_arc)) {
                    return;
                }
            } else {
                // File was deleted during render, just abort the commit.
                return;
            }
        }
    }
    /*
    pub async fn get_rendered_old(&self, file_path: &str) -> anyhow::Result<Value> {
        // 1. Fast path: Check persistent cache
        if let Some(k) = self.inner.files.load().get(file_path) {
            if let Some(r) = &k.rendered {
                return Ok(r.clone());
            }
        }

        // 2. Slow path: Coalesce work
        let future_to_await: InFlightFuture;

        if let Some(entry) = self.inner.in_flight_renders.get(file_path) {
            future_to_await = entry.value().clone();
        } else {
            let self_clone = self.clone();
            let path_owned = file_path.to_string();

            let render_future = async move {
                // The body of the future now returns a `SharedResult`
                let result: anyhow::Result<Value> = async {
                    let global_snapshot = self_clone.inner.files.load();
                    let raw_value = global_snapshot
                        .get(&path_owned)
                        .ok_or_else(|| anyhow!("File not found: {}", path_owned))?
                        .raw
                        .clone();
                    let imports = get_imports(&raw_value);

                    let dep_futures: Vec<_> = imports
                        .iter()
                        .map(|key| self_clone.get_rendered(key))
                        .collect();

                    let dep_results = future::try_join_all(dep_futures).await?;
                    let deps_map: HashMap<String, Value> = imports
                        .into_iter() // Use into_iter to consume the Vec
                        .zip(dep_results)
                        .collect();

                    let mut value_to_render = raw_value;
                    resolve_refs_from_deps(&mut value_to_render, &deps_map);

                    self_clone
                        .commit_single_result(&path_owned, value_to_render.clone())
                        .await;
                    Ok(value_to_render)
                }
                .await;

                // FIX 2: Convert the final `Result<Value, Error>` into a `SharedResult`.
                // If it's an error, wrap it in an Arc.
                result.map_err(Arc::new)
            };

            let boxed_future: Pin<Box<dyn Future<Output = SharedResult> + Send + Sync>> =
                Box::pin(render_future);
            let shared_future = boxed_future.shared();

            match self.inner.in_flight_renders.entry(file_path.to_string()) {
                dashmap::mapref::entry::Entry::Occupied(entry) => {
                    future_to_await = entry.get().clone();
                }
                dashmap::mapref::entry::Entry::Vacant(entry) => {
                    entry.insert(shared_future.clone());
                    future_to_await = shared_future;
                }
            }
        }

        let shared_result = future_to_await.await;
        self.inner.in_flight_renders.remove(file_path);

        // FIX 3: Convert the internal `SharedResult` back to a normal `anyhow::Result`
        // for the public-facing API. We convert the Arc'd error back into a new
        // anyhow::Error, preserving the message.
        shared_result.map_err(|arc_err| anyhow!(arc_err.to_string()))
    }
     */

    /// Recursive helper to perform the actual rendering logic.
    /// It operates on a specific, immutable snapshot of the files.
    fn render_recursive(
        &self,
        file_path: &str,
        rendering_set: &mut HashSet<String>, // Tracks the current call stack for cycle detection
        local_cache: &mut RenderCache, // A temporary "scratchpad" for the current top-level operation
        global_snapshot: &Arc<DagFiles>, // A read-only snapshot of the persistent, shared state
    ) -> anyhow::Result<Value> {
        // --- Step 1: Check all available caches before doing any work ---

        // Priority 1: Check the local cache. We might have already rendered it
        // as a dependency during this *same* top-level `get_rendered` call.
        if let Some(rendered_value) = local_cache.get(file_path) {
            return Ok(rendered_value.clone());
        }

        // Priority 2: Check the global snapshot. It might have been rendered
        // and cached during a *previous* `get_rendered` call.
        let konf = global_snapshot
            .get(file_path)
            .ok_or_else(|| anyhow!("missing file during render: '{}'", file_path))?;
        if let Some(ref rendered_value) = konf.rendered {
            return Ok(rendered_value.clone());
        }

        // --- Step 2: Begin the rendering process (the "slow path") ---

        // First, detect circular dependencies. If we're already in the process of
        // rendering this file in the current call stack, it's a cycle.
        if rendering_set.contains(file_path) {
            return Err(anyhow!(
                "Circular dependency detected involving '{}'",
                file_path
            ));
        }
        // Add the current file to the call stack.
        rendering_set.insert(file_path.to_string());

        // Clone the raw data to have a mutable version to work on.
        let mut value_to_render = konf.raw.clone();

        // --- Step 3: Recursively render all dependencies (imports) first ---

        let imports = get_imports(&value_to_render);
        for import_key in &imports {
            // The recursive call will either return a cached value or render the dependency,
            // populating the `local_cache` in the process. We just need to ensure it succeeds.
            self.render_recursive(import_key, rendering_set, local_cache, global_snapshot)?;
        }

        // --- Step 4: All dependencies are now rendered. Resolve references. ---

        // This function will now be able to find all necessary dependencies by
        // looking in the `local_cache` and `global_snapshot`.
        resolve_refs_from_deps(&mut value_to_render, local_cache);

        // --- Step 5: Finalize and cache the result locally ---

        // Remove the file from the call stack before returning.
        rendering_set.remove(file_path);

        // CRITICAL: Store the newly rendered value in the local cache. This makes the result
        // available to other branches of the render tree in this same operation.
        local_cache.insert(file_path.to_string(), value_to_render.clone());

        Ok(value_to_render)
    }
}
// TODO:
// - reload on sighup
// - get /json|yaml|env/<path> or hash ?

fn get_imports(value: &Value) -> Vec<String> {
    value
        .get("import")
        .and_then(|e| e.as_sequence())
        .map(|seq| {
            seq.iter()
                .filter_map(|e| e.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}
