use std::fmt::Debug;
use thiserror::Error;

use crate::Value;


#[derive(Debug, Clone, Error)]
pub enum LoaderError {
    #[error("Parse failed")]
    ParseFailed,
}

pub trait Loader: Debug + Send + Sync {
    fn ext(&self) -> &'static str;
    fn load(&self, content: &str) -> Result<Value, LoaderError>;
}

#[derive(Debug)]
pub struct MultiLoader {
    pub loaders: Vec<Box<dyn Loader>>,
}

impl MultiLoader {
    pub fn new(loaders: Vec<Box<dyn Loader>>) -> Self {
        Self { loaders }
    }

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

