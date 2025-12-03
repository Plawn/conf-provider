pub mod env;
pub mod json;
pub mod properties;
pub mod toml;
pub mod yaml;
pub mod docker_env;
use std::fmt::Debug;

use crate::Value;

/// Trait for serializing internal `Value` type to various output formats.
pub trait ValueWriter: Debug + Send + Sync {
    /// Returns the format extension this writer handles (e.g., "json", "yaml").
    fn ext(&self) -> &'static str;
    /// Serializes a `Value` to a string representation.
    fn to_str(&self, v: &Value) -> Result<String, WriterError>;
}

/// Error type for serialization failures.
#[derive(Debug, Clone)]
pub struct WriterError {
    pub format: &'static str,
    pub message: String,
}

impl std::fmt::Display for WriterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Failed to serialize to {}: {}", self.format, self.message)
    }
}

impl std::error::Error for WriterError {}

#[derive(Debug)]
pub struct MultiWriter {
    pub loaders: Vec<Box<dyn ValueWriter>>,
}

impl MultiWriter {
    pub fn new(loaders: Vec<Box<dyn ValueWriter>>) -> Self {
        Self { loaders }
    }

    pub fn write(&self, ext: &str, content: &Value) -> Option<Result<String, WriterError>> {
        self.loaders
            .iter()
            .find(|e| ext == e.ext())
            .map(|l| l.to_str(content))
    }
}
