use std::collections::{HashMap, HashSet};
use std::fs;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use konf_provider::loader::{JsonLoader, Loader, MultiLoader, MultiWriter, Value, YamlLoader};
use konf_provider::render::resolve_refs;
use konf_provider::{Konf, utils};
use serde::Deserialize;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use xitca_web::handler::params::Params;
use xitca_web::middleware::tower_http_compat::TowerHttpCompat;
use xitca_web::{
    App,
    handler::{handler_service, json::Json, state::StateRef},
    route::{get, post},
};

#[derive(Debug, Clone)]
struct AppState {
    dag: Arc<Mutex<Dag>>,
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
    let d = Dag::new(folder, multiloader);

    let multiwriter = MultiWriter::new(vec![Box::new(YamlLoader {}), Box::new(JsonLoader {})]);

    let state = Arc::from(AppState {
        dag: Arc::from(Mutex::new(d)),
        writer: Arc::from(multiwriter),
    });

    App::new()
        .with_state(state)
        .at("/live", get(handler_service(async || "OK")))
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
) -> String {
    let d = state.dag.lock().unwrap().get_rendered("c").unwrap();
    let m = &state
        .writer
        .loaders
        .iter()
        .find(|e| e.ext() == format.as_str())
        .unwrap();
    m.to_str(&d)
}

#[derive(Debug)]
pub struct Dag {
    folder: PathBuf,
    multiloader: MultiLoader,
    files: HashMap<String, Konf>,
    rendering: HashSet<String>,
}

impl Dag {
    pub fn new(folder: PathBuf, multiloader: MultiLoader) -> Self {
        let paths = fs::read_dir(&folder).unwrap();
        let mut files: HashMap<String, Konf> = HashMap::new();
        for path in paths {
            let p1 = path.unwrap().file_name();
            let p = p1.to_str().unwrap();
            // bad
            let content =
                fs::read_to_string(String::new() + folder.as_os_str().to_str().unwrap() + "/" + p)
                    .unwrap();
            let s = p.to_string();
            let raw = multiloader.load(&s, &content).unwrap();
            let k = Konf::new(raw);

            let mut pp = s.split(".");
            files.insert(pp.next().unwrap().to_string(), k);
        }

        Self {
            files,
            folder,
            multiloader,
            rendering: HashSet::new(),
        }
    }

    pub fn get_rendered(&mut self, file_path: &str) -> anyhow::Result<Value> {
        if let Some(k) = self.files.get(file_path) {
            if let Some(ref r) = k.rendered {
                return Ok(r.clone());
            }
        }
        self.rendering.insert(file_path.to_string());

        // Get the raw data
        let raw_data = if let Some(k) = self.files.get(file_path) {
            k.raw.clone()
        } else {
            anyhow::bail!("missing");
        };
        let d = raw_data.clone();

        let mut b = raw_data;

        let imports: &Vec<_> = &d
            .get("import") // set const special key
            .cloned()
            .map(|e| {
                e.as_sequence()
                    .unwrap()
                    .iter()
                    .map(|e| e.as_str().unwrap().to_string())
                    .collect()
            })
            .unwrap_or_default();
        for k in imports {
            if self.rendering.contains(k) {
                continue;
            }
            let _ = self.get_rendered(&k)?; // Handle the Result
        }

        resolve_refs(&mut b, &self.files);
        self.set_result(file_path, b.clone());
        self.rendering.remove(file_path);
        Ok(b)
    }

    fn set_result(&mut self, file_path: &str, res: Value) {
        if let Some(k) = self.files.get_mut(file_path) {
            k.rendered = Some(res);
        }
    }

    pub fn get_raw(&self, file_path: &str) -> anyhow::Result<Value> {
        self.files
            .get(file_path)
            .map(|v| v.raw.clone())
            .ok_or_else(|| anyhow::anyhow!("missing"))
    }
}

// TODO:
// - reload on sighup
// - get /json|yaml|env/<path> or hash ?
