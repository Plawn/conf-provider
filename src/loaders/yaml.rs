use std::collections::HashMap;

use crate::{loader::{Loader, LoaderError}, Value};

#[derive(Debug)]
pub struct YamlLoader {}


impl Loader for YamlLoader {
    fn ext(&self) -> &'static str {
        "yaml"
    }

    fn load(&self, content: &str) -> Result<Value, LoaderError> {
        let d: serde_yaml::Value =
            serde_yaml::from_slice(content.as_bytes()).map_err(|_| LoaderError::ParseFailed)?;
        let p = from_yaml(d);
        Ok(p)
    }
}
pub fn from_yaml(yaml_value: serde_yaml::Value) -> Value {
    match yaml_value {
        // Handle strings
        serde_yaml::Value::String(s) => Value::String(s),

        // Handle sequences/arrays
        serde_yaml::Value::Sequence(seq) => {
            let mut vec = Vec::new();
            for item in seq {
                vec.push(from_yaml(item));
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

                hashmap.insert(key_str, from_yaml(value));
            }
            Value::Mapping(hashmap)
        }

        // Convert other types to strings
        serde_yaml::Value::Number(n) => Value::Number(n.as_f64().unwrap_or(0.0)),
        serde_yaml::Value::Bool(b) => Value::String(b.to_string()),
        serde_yaml::Value::Null => Value::Null,

        // Tagged values - extract the inner value
        serde_yaml::Value::Tagged(tagged) => from_yaml(tagged.value),
    }
}
