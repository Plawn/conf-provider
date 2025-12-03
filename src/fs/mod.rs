pub mod local;
pub mod git;

/// Represents a file entry with metadata for configuration loading.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DirEntry {
    /// Relative path without extension (used as config key).
    /// For nested files like `common/base.yaml`, this would be `common/base`.
    pub filename: String,
    /// Full path including filename and extension.
    pub full_path: String,
    /// File extension (used to select appropriate loader).
    pub ext: String,
}

impl DirEntry {
    /// Creates a DirEntry from a path relative to a base directory.
    ///
    /// The `relative_path` should be the path relative to the config root,
    /// e.g., `common/base.yaml` for a file at `/configs/common/base.yaml`.
    pub fn from_relative_path(relative_path: &std::path::Path, full_path: &str) -> Option<Self> {
        let ext = relative_path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();

        // Get the path without extension as the config key
        let filename = relative_path
            .with_extension("")
            .to_string_lossy()
            .into_owned();

        if filename.is_empty() {
            return None;
        }

        Some(DirEntry {
            filename,
            full_path: full_path.to_string(),
            ext,
        })
    }
}

/// Trait for abstracting file system access.
///
/// Implement this trait to support different backends for loading
/// configuration files (local filesystem, git repository, S3, etc.).
pub trait FileProvider {
    /// Loads the content of a file at the given path.
    ///
    /// Returns `None` if the file doesn't exist or can't be read.
    fn load(&self, path: &str) -> impl std::future::Future<Output = Option<String>> + Send;

    /// Lists all available configuration files.
    fn list(&self) -> impl std::future::Future<Output = Vec<DirEntry>> + Send;
}
