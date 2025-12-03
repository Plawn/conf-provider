use std::path::{Path, PathBuf};

use crate::fs::{DirEntry, FileProvider};

#[derive(Clone, Debug)]
pub struct BasicFsFileProvider {
    folder: PathBuf,
}

impl BasicFsFileProvider {
    pub fn new(folder: PathBuf) -> Self {
        Self { folder }
    }

    /// Recursively lists all files in a directory.
    async fn list_recursive(
        base: &Path,
        current: &Path,
        entries: &mut Vec<DirEntry>,
    ) {
        let Ok(mut dir) = tokio::fs::read_dir(current).await else {
            return;
        };

        while let Ok(Some(entry)) = dir.next_entry().await {
            let path = entry.path();

            if path.is_dir() {
                // Recursively process subdirectories
                Box::pin(Self::list_recursive(base, &path, entries)).await;
            } else if path.is_file() {
                // Calculate relative path from base folder
                if let Ok(relative) = path.strip_prefix(base) {
                    let full_path = path.to_string_lossy().into_owned();
                    if let Some(dir_entry) = DirEntry::from_relative_path(relative, &full_path) {
                        entries.push(dir_entry);
                    }
                }
            }
        }
    }
}

impl FileProvider for BasicFsFileProvider {
    async fn load(&self, path: &str) -> Option<String> {
        tokio::fs::read_to_string(path).await.ok()
    }

    async fn list(&self) -> Vec<DirEntry> {
        let mut entries = Vec::new();
        Self::list_recursive(&self.folder, &self.folder, &mut entries).await;
        entries
    }
}
