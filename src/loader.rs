use std::fmt::Debug;
use thiserror::Error;

use crate::Value;

/// Error type for configuration loading/parsing failures.
#[derive(Debug, Clone, Error)]
pub enum LoaderError {
    /// The configuration file could not be parsed.
    #[error("Parse failed")]
    ParseFailed,
}

/// Trait for loading configuration files from string content.
///
/// Implement this trait to add support for new configuration formats.
/// Each loader handles a specific file extension.
pub trait Loader: Debug + Send + Sync {
    /// Returns the file extension this loader handles (e.g., "yaml", "json").
    fn ext(&self) -> &'static str;
    /// Parses the given content string into a `Value`.
    fn load(&self, content: &str) -> Result<Value, LoaderError>;
}

/// A collection of loaders that dispatches to the appropriate one based on file extension.
#[derive(Debug)]
pub struct MultiLoader {
    pub loaders: Vec<Box<dyn Loader>>,
}

impl MultiLoader {
    /// Creates a new `MultiLoader` with the given loaders.
    pub fn new(loaders: Vec<Box<dyn Loader>>) -> Self {
        Self { loaders }
    }

    /// Loads content using the loader that matches the given extension.
    ///
    /// Returns `LoaderError::ParseFailed` if no loader handles the extension.
    pub fn load(&self, ext: &str, content: &str) -> Result<Value, LoaderError> {
        let l = self
            .loaders
            .iter()
            .find(|e| ext == e.ext());
        if let Some(loader) = l {
            return loader.load(content);
        }
        Err(LoaderError::ParseFailed)
    }
}

