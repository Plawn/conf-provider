use anyhow::anyhow;
use arc_swap::ArcSwap;
use konf_provider::loader::{JsonLoader, MultiLoader, MultiWriter, Value, YamlLoader};
use konf_provider::render::resolve_refs_with_cache;
use konf_provider::utils::MyError;
use konf_provider::{DagFiles, Konf, RenderCache, utils};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
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
    let d = state
        .dag
        .get_rendered("c")
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
pub struct Dag {
    // Stable configuration
    folder: PathBuf,
    multiloader: MultiLoader,

    // The dynamically reloaded, shared state
    files: ArcSwap<DagFiles>,
}

impl Dag {
    // new() now initializes the ArcSwap with the first load
    pub fn new(folder: PathBuf, multiloader: MultiLoader) -> anyhow::Result<Self> {
        let s = Self {
            folder,
            multiloader,
            files: ArcSwap::default(), // Start with an empty HashMap
        };
        s.reload()?; // Perform the initial load
        Ok(s)
    }

    // reload() now takes &self and updates the ArcSwap
    pub fn reload(&self) -> anyhow::Result<()> {
        let paths = fs::read_dir(&self.folder)?;
        let mut files: DagFiles = HashMap::new();
        for path in paths {
            let p1 = path?.file_name();
            let p = p1.to_str().ok_or(anyhow!("failed to read filename"))?;
            let mut m_folder = self.folder.clone();
            m_folder.push(p);
            let content = fs::read_to_string(m_folder)?;
            let k = Konf::new(self.multiloader.load(&p, &content)?);

            let path_no_ext = p.split('.').next().ok_or(anyhow!("invalid path"))?;
            files.insert(path_no_ext.to_string(), k);
        }

        // Atomically publish the new HashMap
        self.files.store(Arc::new(files));
        Ok(())
    }

    pub fn get_raw(&self, file_path: &str) -> anyhow::Result<Value> {
        // Load the current snapshot of files to perform the read
        let files_snapshot = self.files.load();
        files_snapshot
            .get(file_path)
            .map(|v| v.raw.clone())
            .ok_or_else(|| anyhow!("missing raw file: {}", file_path))
    }

    pub fn get_rendered(&self, file_path: &str) -> anyhow::Result<Value> {
        // --- Fast path: Check if already rendered in the current snapshot ---
        let files_snapshot = self.files.load();
        if let Some(k) = files_snapshot.get(file_path) {
            if let Some(ref r) = k.rendered {
                return Ok(r.clone());
            }
        }
        drop(files_snapshot);

        // --- Slow path: Render and cache the result ---
        let mut rendering = HashSet::new();
        // The render logic itself can operate on a momentary, consistent snapshot.
        let mut render_cache = HashMap::new();
        let rendered_value = self.render_recursive(
            file_path,
            &mut rendering,
            &mut render_cache,
            &self.files.load(),
        )?;

        // --- Cache the result using the correct "compare-and-swap" (CAS) loop ---
        loop {
            // 1. Load the current Arc/Guard. This is our baseline.
            let current_guard = self.files.load();

            // 2. Create the new state based on the current one.
            let mut new_map = (**current_guard).clone();
            if let Some(k) = new_map.get_mut(file_path) {
                k.rendered = Some(rendered_value.clone());
            } else {
                return Err(anyhow!("File '{}' disappeared during render", file_path));
            }
            let new_arc = Arc::new(new_map);

            // 3. Attempt the swap. The function returns the value that was in there
            //    *just before* our operation.
            let previous_guard = self.files.compare_and_swap(&current_guard, new_arc);

            // 4. Check for success using pointer equality.
            if Arc::ptr_eq(&current_guard, &previous_guard) {
                // SUCCESS: The `previous_guard` points to the *same memory* as our
                // `current_guard`. This means no other thread changed the value
                // between our `load` and our `compare_and_swap`. Our update is now committed.
                break;
            }

            // FAILURE: Another thread must have called `store` or a successful `compare_and_swap`.
            // The value we got back is different from our baseline. Our `new_map` is stale.
            // The loop will now repeat, starting from step 1 with the newer state.
        }

        Ok(rendered_value)
    }

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
        resolve_refs_with_cache(&mut value_to_render, local_cache, global_snapshot);

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
