//! Workspace management for konf-lsp
//!
//! Handles indexing and caching of konf config files in the workspace.
//! Uses `.konf` marker files to determine the root for relative paths.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tower_lsp::lsp_types::Url;
use tracing::{info, warn};
use walkdir::WalkDir;

use crate::parser::KonfDocument;

/// The marker file that indicates a konf config root
const KONF_MARKER: &str = ".konf";

/// Manages the workspace state and indexed documents
#[derive(Debug, Default)]
pub struct Workspace {
    /// Root folders of the workspace (from VSCode)
    workspace_folders: Vec<PathBuf>,
    /// Konf roots (directories containing .konf files)
    konf_roots: Vec<PathBuf>,
    /// Indexed documents by URI
    documents: HashMap<String, KonfDocument>,
    /// Map from config key to URI (e.g., "common/database" -> "file:///path/to/common/database.yaml")
    key_to_uri: HashMap<String, String>,
}

impl Workspace {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a workspace folder and index its YAML files
    pub fn add_folder(&mut self, uri: &Url) {
        let Ok(path) = uri.to_file_path() else {
            warn!("Could not convert URI to path: {}", uri);
            return;
        };

        info!("Adding workspace folder: {}", path.display());
        self.workspace_folders.push(path.clone());

        // Find all .konf marker files in this folder
        self.find_konf_roots(&path);

        // Index YAML files
        self.index_folder(&path);
    }

    /// Find all .konf marker files and register their directories as konf roots
    fn find_konf_roots(&mut self, root: &Path) {
        for entry in WalkDir::new(root)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.file_name().is_some_and(|n| n == KONF_MARKER) {
                if let Some(parent) = path.parent() {
                    info!("Found konf root: {}", parent.display());
                    self.konf_roots.push(parent.to_path_buf());
                }
            }
        }

        // Sort by path length descending so we match the most specific root first
        self.konf_roots.sort_by(|a, b| b.as_os_str().len().cmp(&a.as_os_str().len()));
    }

    /// Index all YAML files in a folder
    fn index_folder(&mut self, root: &Path) {
        for entry in WalkDir::new(root)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();

            // Only process YAML files
            if !is_yaml_file(path) {
                continue;
            }

            // Read and parse the file
            if let Ok(content) = std::fs::read_to_string(path) {
                let key = self.path_to_key(path);
                let uri = Url::from_file_path(path)
                    .map(|u| u.to_string())
                    .unwrap_or_default();

                info!("Indexed: {} -> {}", key, uri);

                let doc = KonfDocument::parse(key.clone(), content);
                self.key_to_uri.insert(key, uri.clone());
                self.documents.insert(uri, doc);
            }
        }
    }

    /// Convert a file path to a konf config key
    /// Uses the nearest .konf root as the base for relative paths
    /// e.g., /workspace/configs/common/database.yaml -> common/database (if .konf is in /workspace/configs)
    fn path_to_key(&self, path: &Path) -> String {
        // First try to find a konf root that contains this path
        for konf_root in &self.konf_roots {
            if let Ok(relative) = path.strip_prefix(konf_root) {
                let key = relative
                    .with_extension("")
                    .to_string_lossy()
                    .replace('\\', "/");
                return key;
            }
        }

        // Fallback to workspace folder root
        for root in &self.workspace_folders {
            if let Ok(relative) = path.strip_prefix(root) {
                let key = relative
                    .with_extension("")
                    .to_string_lossy()
                    .replace('\\', "/");
                return key;
            }
        }

        // Final fallback: use filename without extension
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string()
    }

    /// Update a document's content (called on didOpen/didChange)
    pub fn update_document(&mut self, uri: &Url, content: &str) {
        let uri_str = uri.to_string();
        let key = self.uri_to_key(uri);

        let doc = KonfDocument::parse(key.clone(), content.to_string());
        self.key_to_uri.insert(key, uri_str.clone());
        self.documents.insert(uri_str, doc);
    }

    /// Convert a URI to a konf config key
    fn uri_to_key(&self, uri: &Url) -> String {
        if let Ok(path) = uri.to_file_path() {
            self.path_to_key(&path)
        } else {
            uri.path().to_string()
        }
    }

    /// Get a document by URI
    pub fn get_document(&self, uri: &Url) -> Option<&KonfDocument> {
        self.documents.get(&uri.to_string())
    }

    /// Get a document by its config key
    pub fn get_document_by_key(&self, key: &str) -> Option<&KonfDocument> {
        let uri = self.key_to_uri.get(key)?;
        self.documents.get(uri)
    }

    /// Get the URI for a config key
    pub fn get_uri_for_key(&self, key: &str) -> Option<&String> {
        self.key_to_uri.get(key)
    }

    /// Get all available config keys
    pub fn get_all_keys(&self) -> Vec<&String> {
        self.key_to_uri.keys().collect()
    }

    /// Get all documents
    #[allow(dead_code)]
    pub fn get_all_documents(&self) -> impl Iterator<Item = &KonfDocument> {
        self.documents.values()
    }

    /// Check if a config key exists
    pub fn has_key(&self, key: &str) -> bool {
        self.key_to_uri.contains_key(key)
    }
}

/// Check if a path is a YAML file
fn is_yaml_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext == "yaml" || ext == "yml")
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_yaml_file() {
        assert!(is_yaml_file(Path::new("config.yaml")));
        assert!(is_yaml_file(Path::new("config.yml")));
        assert!(!is_yaml_file(Path::new("config.json")));
        assert!(!is_yaml_file(Path::new("config")));
    }
}
