use std::collections::{HashMap, HashSet};

use crate::{Value, fs::FileProvider, loader::MultiLoader};

#[derive(Debug)]
pub struct Authorizer {
    paths: HashMap<String, HashSet<String>>, // path -> set(auth tokens)
}

impl Authorizer {
    pub fn authorize(&self, path: &str, token: &str) -> bool {
        self.paths
            .get(path)
            .map(|tokens| tokens.contains(token))
            .unwrap_or(false)
    }

    pub async fn new<P: FileProvider>(fs: &P, loader: &MultiLoader) -> Self {
        let mut paths: HashMap<String, HashSet<String>> = HashMap::new();
        for path in fs.list().await {
            if let Some(content) = fs.load(&path.full_path).await {
                let p = loader.load(&path.ext, &content).expect("faield to parse");
                if let Some(a) = p.get("auth") {
                    match a {
                        Value::Sequence(values) => {
                            for i in values
                                .into_iter()
                                .filter_map(|e| e.as_str())
                            {
                                match paths.entry(path.filename.clone()) {
                                    std::collections::hash_map::Entry::Occupied(
                                        mut occupied_entry,
                                    ) => {
                                        occupied_entry.get_mut().insert(i.clone());
                                    }
                                    std::collections::hash_map::Entry::Vacant(vacant_entry) => {
                                        let mut s = HashSet::new();
                                        s.insert(i.clone());
                                        vacant_entry.insert(s);
                                    }
                                }
                            }
                        }
                        _ => panic!("invalid type"),
                    }
                }
            }
        }
        Self { paths }
    }
}
