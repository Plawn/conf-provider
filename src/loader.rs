use serde::Serialize;
use std::collections::HashMap;
use std::fmt::Debug;
use thiserror::Error;

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

pub trait ValueWriter: Debug + Send + Sync {
    fn ext(&self) -> &'static str;
    fn to_str(&self, v: &Value) -> String;
}

#[derive(Debug)]
pub struct YamlLoader {}

#[derive(Debug)]
pub struct JsonLoader {}

impl Loader for YamlLoader {
    fn ext(&self) -> &'static str {
        "yaml"
    }

    fn load(&self, content: &str) -> Result<Value, LoaderError> {
        let d: serde_yaml::Value =
            serde_yaml::from_slice(content.as_bytes()).map_err(|_| LoaderError::ParseFailed)?;
        let p = Value::from_yaml(d);
        Ok(p)
    }
}

impl ValueWriter for JsonLoader {
    fn ext(&self) -> &'static str {
        "json"
    }
    fn to_str(&self, v: &Value) -> String {
        serde_json::to_string(&Value::to_json(v)).unwrap()
    }
}

impl ValueWriter for YamlLoader {
    fn ext(&self) -> &'static str {
        "yaml"
    }
    fn to_str(&self, v: &Value) -> String {
        serde_yaml::to_string(&Value::to_yaml(v)).unwrap()
    }
}

impl Value {
    /// Convert from serde_yaml::Value to internal Value
    pub fn from_yaml(yaml_value: serde_yaml::Value) -> Value {
        match yaml_value {
            // Handle strings
            serde_yaml::Value::String(s) => Value::String(s),

            // Handle sequences/arrays
            serde_yaml::Value::Sequence(seq) => {
                let mut vec = Vec::new();
                for item in seq {
                    vec.push(Value::from_yaml(item));
                }
                Value::Sequence(vec)
            }

            // Handle mappings/objects
            serde_yaml::Value::Mapping(map) => {
                let mut hashmap = HashMap::new();
                for (key, value) in map {
                    // Convert key to string
                    let key_str = match key {
                        serde_yaml::Value::String(s) => s,
                        serde_yaml::Value::Number(n) => n.to_string(),
                        serde_yaml::Value::Bool(b) => b.to_string(),
                        _ => continue, // Skip non-convertible keys
                    };

                    hashmap.insert(key_str, Value::from_yaml(value));
                }
                Value::Mapping(hashmap)
            }

            // Convert other types to strings
            serde_yaml::Value::Number(n) => Value::Number(n.as_f64().unwrap()),
            serde_yaml::Value::Bool(b) => Value::String(b.to_string()),
            serde_yaml::Value::Null => Value::Null,

            // Tagged values - extract the inner value
            serde_yaml::Value::Tagged(tagged) => Value::from_yaml(tagged.value),
        }
    }

    /// Convert from internal Value back to serde_yaml::Value
    pub fn to_yaml(&self) -> serde_yaml::Value {
        match self {
            Value::Number(n) => serde_yaml::Value::Number(serde_yaml::Number::from(n.clone())),
            Value::String(s) => serde_yaml::Value::String(s.clone()),
            Value::Boolean(b) => serde_yaml::Value::Bool(b.clone()),
            Value::Null => serde_yaml::Value::Null,
            Value::Sequence(seq) => {
                let yaml_seq: Vec<serde_yaml::Value> = seq.iter().map(|v| v.to_yaml()).collect();
                serde_yaml::Value::Sequence(yaml_seq)
            }
            Value::Mapping(map) => {
                let mut yaml_map = serde_yaml::Mapping::new();
                for (key, value) in map {
                    yaml_map.insert(serde_yaml::Value::String(key.clone()), value.to_yaml());
                }
                serde_yaml::Value::Mapping(yaml_map)
            }
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        use serde_json::{Map, Value as JsonValue};
        match self {
            Value::Number(n) => serde_json::Number::from_f64(*n)
                .map(JsonValue::Number)
                .unwrap_or(JsonValue::Null),
            Value::String(s) => serde_json::Value::String(s.clone()),
            Value::Boolean(b) => serde_json::Value::Bool(b.clone()),
            Value::Null => serde_json::Value::Null,
            Value::Sequence(seq) => {
                let arr: Vec<JsonValue> = seq.iter().map(|v| v.to_json()).collect();
                JsonValue::Array(arr)
            }
            Value::Mapping(map) => {
                let mut obj = Map::new();
                for (key, value) in map {
                    // assuming your keys are always strings in `Value::Mapping`
                    obj.insert(key.clone(), value.to_json());
                }
                JsonValue::Object(obj)
            }
        }
    }
}

#[derive(Debug)]
pub struct MultiLoader {
    pub loaders: Vec<Box<dyn Loader>>,
}

impl MultiLoader {
    pub fn new(loaders: Vec<Box<dyn Loader>>) -> Self {
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
