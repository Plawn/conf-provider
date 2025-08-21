pub mod fs;
pub mod git;

#[derive(Clone, Debug)]
pub struct DirEntry {
    /// Without extension
    pub filename: String,
    /// Full path, with the filename and the ext
    pub full_path: String,
    /// Extension of the file
    pub ext: String,
}

pub trait FileProvider {
    async fn load(&self, path: &str) -> Option<String>;
    async fn list(&self) -> Vec<DirEntry>;
}
