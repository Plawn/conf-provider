#![feature(error_generic_member_access, error_reporter)]

use std::{collections::HashMap, pin::Pin, sync::Arc};

use futures::future::Shared;

use crate::loader::Value;

pub mod utils;

pub mod loader;

pub mod render;

#[derive(Debug, Clone)]
pub struct Konf {
    pub raw: Value,
    pub rendered: Option<Value>,
}

impl Konf {
    pub fn new(raw: Value) -> Self {
        Self {
            raw,
            rendered: None,
        }
    }
}

pub type DagFiles = HashMap<String, Konf>;
pub type RenderCache = HashMap<String, Value>;
pub type SharedResult = Result<Value, Arc<anyhow::Error>>;
pub type InFlightFuture = Shared<Pin<Box<dyn Future<Output = SharedResult> + Send + Sync>>>;