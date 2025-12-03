use std::collections::{HashMap, HashSet};

use crate::{fs::FileProvider, loader::MultiLoader, utils::get_conf_strings};

/// Token-based authorizer for controlling access to configuration files.
///
/// In git mode, each configuration file can specify which tokens are allowed
/// to access it using the `auth` key in the `<!>` metadata section:
///
/// ```yaml
/// <!>:
///   auth:
///     - token1
///     - token2
/// ```
#[derive(Debug)]
pub struct Authorizer {
    /// Maps file paths to the set of tokens allowed to access them.
    paths: HashMap<String, HashSet<String>>,
}

impl Authorizer {
    /// Checks if the given token is authorized to access the file at `path`.
    ///
    /// Returns `false` if the path has no authorization configured or the token is not in the allowed list.
    pub fn authorize(&self, path: &str, token: &str) -> bool {
        self.paths
            .get(path)
            .map(|tokens| tokens.contains(token))
            .unwrap_or(false)
    }

    /// Creates a new authorizer by scanning all files for auth configurations.
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
                    Err(_) => {
                        tracing::warn!("failed to read {:?}", &path);
                    }
                }
            }
        }
        Self { paths }
    }
}
