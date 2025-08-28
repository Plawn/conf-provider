use std::collections::{HashMap, HashSet};

use crate::{fs::FileProvider, loader::MultiLoader, utils::get_conf_strings};

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
        const IMPORT_KEY: &str = "auth";
        let mut paths: HashMap<String, HashSet<String>> = HashMap::new();
        for path in fs.list().await {
            if let Some(content) = fs.load(&path.full_path).await {
                match loader.load(&path.ext, &content) {
                    Ok(p) => {
                        let values = get_conf_strings(&p, IMPORT_KEY);
                        for i in values.iter() {
                            match paths.entry(path.filename.clone()) {
                                std::collections::hash_map::Entry::Occupied(mut occupied_entry) => {
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
                    Err(_) => todo!(),
                }
            }
        }
        Self { paths }
    }
}
