#![feature(error_generic_member_access, error_reporter)]

use std::{collections::HashMap, pin::Pin, sync::Arc};

use futures::future::Shared;

use crate::loader::Value;
use async_once_cell::OnceCell;
pub mod utils;

pub mod loader;

pub mod render;

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

pub type DagFiles = HashMap<String, Konf>;
pub type RenderCache = HashMap<String, Value>;
pub type SharedResult = Result<Value, Arc<anyhow::Error>>;
pub type InFlightFuture = Shared<Pin<Box<dyn Future<Output = SharedResult> + Send + Sync>>>;