use serde::Serialize;
use std::collections::HashMap;
use std::fmt::Debug;
use thiserror::Error;

use crate::writer::ValueWriter;

#[derive(Debug, Clone, Serialize)]
pub enum Value {
    String(String),
    Sequence(Sequence),
    Mapping(HashMap<String, Value>),
    Number(f64),
    Boolean(bool),
    Null,
}

pub type Sequence = Vec<Value>;

impl Value {
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Mapping(hash_map) => hash_map.get(key),
            _ => None,
        }
    }

    pub fn as_sequence(&self) -> Option<&Sequence> {
        match self {
            Value::Sequence(values) => Some(values),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&String> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }
}

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
pub struct MultiLoader<L>
where
    L: Loader,
{
    pub loaders: Vec<L>,
}

impl<L: Loader> MultiLoader<L> {
    pub fn new(loaders: Vec<L>) -> Self {
        Self { loaders }
    }

    pub fn load(&self, filename: &str, content: &str) -> Result<Value, LoaderError> {
        let l = self
            .loaders
            .iter()
            .find(|e| filename.split(".").last().unwrap() == e.ext());
        if let Some(loader) = l {
            return loader.load(content);
        }
        Err(LoaderError::ParseFailed)
    }
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
