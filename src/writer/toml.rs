use crate::{Value, writer::{ValueWriter, WriterError}};
use std::collections::BTreeMap;

#[derive(Debug)]
pub struct TomlWriter {}

impl ValueWriter for TomlWriter {
    fn ext(&self) -> &'static str {
        "toml"
    }

    fn to_str(&self, v: &Value) -> Result<String, WriterError> {
        const ROOT_KEY: &str = "root";
        let toml_value = to_toml(v);
        let map_err = |e: toml::ser::Error| WriterError {
            format: "toml",
            message: e.to_string(),
        };
        // The toml crate expects a top-level table for serialization.
        // If our value is not a mapping, we'll wrap it in a table with a default key.
        if let toml::Value::Table(table) = toml_value {
            toml::to_string_pretty(&table).map_err(map_err)
        } else {
            let mut table = BTreeMap::new();
            table.insert(ROOT_KEY, toml_value);
            toml::to_string_pretty(&table).map_err(map_err)
        }
    }
}

/// Convert from internal Value back to toml::Value
pub fn to_toml(value: &Value) -> toml::Value {
    match value {
        Value::Number(n) => toml::Value::Float(*n),
        Value::String(s) => toml::Value::String(s.clone()),
        Value::Boolean(b) => toml::Value::Boolean(*b),
        Value::Null => toml::Value::String("".to_string()), // TOML doesn't have a null type, representing as empty string
        Value::Sequence(seq) => {
            let toml_seq: Vec<toml::Value> = seq.iter().map(to_toml).collect();
            toml::Value::Array(toml_seq)
        }
        Value::Mapping(map) => {
            let mut toml_map = BTreeMap::new();
            for (key, value) in map {
                toml_map.insert(key.clone(), to_toml(value));
            }
            toml::Value::Table(toml_map.into_iter().collect())
        }
    }
}

impl TomlWriter {
    pub fn new_boxed() -> Box<Self> {
        Box::new(Self {})
    }
}
