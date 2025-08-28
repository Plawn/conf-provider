#![feature(error_generic_member_access, error_reporter)]
use std::{collections::HashMap, pin::Pin, sync::Arc};

use futures::future::Shared;


use async_once_cell::OnceCell;
use serde::Serialize;

use crate::{authorizer::Authorizer, fs::FileProvider, render::Dag};
pub mod utils;
pub mod writer;
pub mod loaders;
pub mod loader;
pub mod fs;
pub mod render_helper;
pub mod render;
pub mod authorizer;
pub mod git_routes;
pub mod local_routes;
pub mod config;
#[derive(Debug)]
pub struct Konf {
    pub raw: Value,
    pub rendered: OnceCell<Value>,
}

impl Konf {
    pub fn new(raw: Value) -> Self {
        Self {
            raw,
            rendered: OnceCell::new(),
        }
    }
}

#[derive(Debug)]
pub struct DagEntry<P: FileProvider> {
    pub dag: Dag<P>,
    pub authorizer: Authorizer,
}

#[derive(Debug, Clone, Serialize)]
pub enum Value {
    String(String),
    Sequence(Sequence),
    Mapping(Mapping),
    Number(f64),
    Boolean(bool),
    Null,
}

pub type Sequence = Vec<Value>;
pub type Mapping = HashMap<String, Value>;

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

     pub fn as_mapping(&self) -> Option<&Mapping> {
        match self {
            Value::Mapping(values) => Some(values),
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

pub type DagFiles = HashMap<String, Konf>;
pub type RenderCache = HashMap<String, Value>;
pub type SharedResult = Result<Value, Arc<anyhow::Error>>;
pub type InFlightFuture = Shared<Pin<Box<dyn Future<Output = SharedResult> + Send + Sync>>>;