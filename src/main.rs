use std::collections::{HashMap, HashSet};
use std::fs;

use std::path::PathBuf;
use std::sync::Arc;

use konf_provider::utils;
use serde_yaml::Value;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use xitca_web::middleware::tower_http_compat::TowerHttpCompat;
use xitca_web::{
    App,
    handler::{handler_service, json::Json, state::StateRef},
    route::{get, post},
};

struct AppState {}

// fn main() -> std::io::Result<()> {
//     tracing_subscriber::registry()
//         .with(
//             tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
//                 format!(
//                     "{}=debug,tower_http=debug,axum::rejection=trace",
//                     env!("CARGO_CRATE_NAME")
//                 )
//                 .into()
//             }),
//         )
//         .with(tracing_subscriber::fmt::layer())
//         .init();
//     let state = Arc::from(AppState {});
//     App::new()
//         .with_state(state)
//         .at("/live", get(handler_service(async || "OK")))
//         .enclosed_fn(utils::error_handler)
//         .enclosed(TowerHttpCompat::new(TraceLayer::new_for_http()))
//         .serve()
//         .bind(format!("0.0.0.0:{}", &4000))?
//         .run()
//         .wait()
// }

struct FileUpdate {
    path: String,
    content: String,
}

struct NewCommit {
    updates: Vec<FileUpdate>,
}

fn get_file(file_path: String) {}

#[derive(Debug, Clone)]
struct Konf {
    raw: Value,
    rendered: Option<Value>,
}

#[derive(Debug)]
struct Dag {
    files: HashMap<String, Konf>,
    rendering: HashSet<String>,
}

/// Lookup a dotted path like "country.city" inside a nested JSON object.
fn lookup_path<'a>(dag: &'a HashMap<String, Konf>, path: &str) -> Option<Value> {
    let mut s = path.split('.');
    let file_key = s.next().expect("not root key");
    let k = dag.get(file_key).expect("missing data");
    let root = k.rendered.clone().expect("not rendered");
    let mut current = root;
    for key in s {
        match current {
            Value::Mapping(map) => {
                current = map.get(key).cloned()?;
            }
            _ => return None,
        }
    }
    Some(current)
}

/// Traverse a JSON `Value` and replace any `"#ref:<path>"` with the corresponding value in `map`.
pub fn resolve_refs(value: &mut Value, map: &HashMap<String, Konf>) {
    println!("resolving {:?}", &value);
    match value {
        Value::String(s) => {
            if let Some(path) = s.strip_prefix("#ref:") {
                if let Some(replacement) = lookup_path(map, path) {
                    *value = replacement.clone();
                }
            }
        }
        Value::Sequence(arr) => {
            for v in arr {
                resolve_refs(v, map);
            }
        }
        Value::Mapping(obj) => {
            for (_k, v) in obj.iter_mut() {
                resolve_refs(v, map);
            }
        }
        _ => {} // numbers, bools, null → nothing to do
    }
}
impl Dag {
    pub fn new(folder: &PathBuf) -> Self {
        let paths = fs::read_dir(folder).unwrap();
        let mut files: HashMap<String, Konf> = HashMap::new();
        for path in paths {
            let p1 = path.unwrap().file_name();
            let p = p1.to_str().unwrap();
            println!("Name: {:?}", p);
            // bad
            let content =
                fs::read_to_string(String::new() + folder.as_os_str().to_str().unwrap() + "/" + p)
                    .unwrap();
            let raw: Value = serde_yaml::from_slice(content.as_bytes()).unwrap();
            let k = Konf {
                raw,
                rendered: None,
            };
            let s = p.to_string();
            let mut pp = s.split(".");
            files.insert(pp.next().unwrap().to_string(), k);
        }
        Self {
            files,
            rendering: HashSet::new(),
        }
    }

    pub fn get_rendered(&mut self, file_path: &str) -> anyhow::Result<Value> {
        println!("renderign {}", file_path);
        // Check if already rendered first
        // println!("renderering {}", file_path);
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
            .get("import")
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

// read file
// update file

// keep all changes to create a commit somewhere

fn main() {
    let folder = "example".parse::<PathBuf>().unwrap();

    let mut d = Dag::new(&folder);
    println!("Dag: {:?}", d);
    println!("C: {:?}", d.get_rendered("c"))
}
