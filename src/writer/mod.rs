pub mod env;
pub mod json;
pub mod properties;
pub mod toml;
pub mod yaml;

use std::fmt::Debug;

use crate::Value;
pub trait ValueWriter: Debug + Send + Sync {
    fn ext(&self) -> &'static str;
    fn to_str(&self, v: &Value) -> String;
}

#[derive(Debug)]
pub struct MultiWriter {
    pub loaders: Vec<Box<dyn ValueWriter>>,
}

impl MultiWriter {
    pub fn new(loaders: Vec<Box<dyn ValueWriter>>) -> Self {
        Self { loaders }
    }

    pub fn write(&self, ext: &str, content: &Value) -> Option<String> {
        self.loaders
            .iter()
            .find(|e| ext == e.ext())
            .map(|l| l.to_str(content))
    }
}
