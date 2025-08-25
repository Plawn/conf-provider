pub mod local;
pub mod git;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DirEntry {
    /// Without extension
    pub filename: String,
    /// Full path, with the filename and the ext
    pub full_path: String,
    /// Extension of the file
    pub ext: String,
}

// TODO: should read using a token which is valid for the given prefix

pub trait FileProvider {
    fn load(&self, path: &str) -> impl std::future::Future<Output = Option<String>> + Send;
    fn list(&self) -> impl std::future::Future<Output = Vec<DirEntry>> + Send;
}
