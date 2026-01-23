#![feature(error_generic_member_access, error_reporter)]
use std::collections::HashMap;

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
pub mod metrics;
pub mod telemetry;
pub mod functions;
pub mod imports;
/// A configuration entry that holds both raw and rendered versions.
///
/// The `raw` field contains the original parsed configuration, while
/// `rendered` is lazily populated with the fully resolved version after
/// template variables have been substituted.
#[derive(Debug)]
pub struct Konf {
    /// The original parsed configuration value before template resolution.
    pub raw: Value,
    /// Lazily computed rendered value with all template variables resolved.
    pub rendered: OnceCell<Value>,
}

impl Konf {
    /// Creates a new `Konf` with the given raw value.
    pub fn new(raw: Value) -> Self {
        Self {
            raw,
            rendered: OnceCell::new(),
        }
    }
}

/// A cache entry combining a DAG with its associated authorizer.
/// Used in git mode to cache per-commit configurations.
#[derive(Debug)]
pub struct DagEntry<P: FileProvider> {
    /// The configuration DAG for this entry.
    pub dag: Dag<P>,
    /// The authorizer controlling access to configurations.
    pub authorizer: Authorizer,
}

/// Internal representation of configuration values.
///
/// This enum provides a format-agnostic representation that can be
/// loaded from various formats (YAML, JSON, etc.) and serialized
/// to multiple output formats.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum Value {
    /// A string value.
    String(String),
    /// An ordered list of values.
    Sequence(Sequence),
    /// A key-value mapping (object/dictionary).
    Mapping(Mapping),
    /// An integer numeric value.
    Int(i64),
    /// A floating-point numeric value.
    Float(f64),
    /// A boolean value.
    Boolean(bool),
    /// A null/empty value.
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