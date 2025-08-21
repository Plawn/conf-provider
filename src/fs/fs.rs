use std::path::PathBuf;

use crate::fs::{DirEntry, FileProvider};



impl TryFrom<tokio::fs::DirEntry> for DirEntry {
    type Error = std::io::Error;

    fn try_from(entry: tokio::fs::DirEntry) -> Result<Self, Self::Error> {
        let path = entry.path();
        let filename = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid filename")
            })?
            .to_string();
        let full_path = path.to_string_lossy().into_owned();
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        Ok(DirEntry {
            filename,
            full_path,
            ext,
        })
    }
}

#[derive(Clone, Debug)]
pub struct BasicFsFileProvider {
    folder: PathBuf,
}

impl BasicFsFileProvider {
    pub fn new(folder: PathBuf) -> Self {
        Self { folder }
    }
}

impl FileProvider for BasicFsFileProvider {
    async fn load(&self, path: &str) -> Option<String> {
        let content = tokio::fs::read_to_string(path).await.ok();
        content
    }

    async fn list(&self) -> Vec<DirEntry> {
        let mut entries = Vec::new();
        if let Some(mut dir) = tokio::fs::read_dir(&self.folder).await.ok() {
            while let Ok(Some(entry)) = dir.next_entry().await {
                if let Ok(e) = entry.try_into() {
                    entries.push(e);
                }
                // could do a warning
            }
        }
        entries
    }
}
