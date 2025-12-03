use crate::{writer::{ValueWriter, WriterError}, Value};

#[derive(Debug)]
pub struct YamlWriter {}

impl ValueWriter for YamlWriter {
    fn ext(&self) -> &'static str {
        "yaml"
    }
    fn to_str(&self, v: &Value) -> Result<String, WriterError> {
        serde_yaml::to_string(&to_yaml(v)).map_err(|e| WriterError {
            format: "yaml",
            message: e.to_string(),
        })
    }
}

/// Convert from internal Value back to serde_yaml::Value
pub fn to_yaml(value: &Value) -> serde_yaml::Value {
    match value {
        Value::Number(n) => serde_yaml::Value::Number(serde_yaml::Number::from(*n)),
        Value::String(s) => serde_yaml::Value::String(s.clone()),
        Value::Boolean(b) => serde_yaml::Value::Bool(*b),
        Value::Null => serde_yaml::Value::Null,
        Value::Sequence(seq) => {
            let yaml_seq: Vec<serde_yaml::Value> = seq.iter().map(to_yaml).collect();
            serde_yaml::Value::Sequence(yaml_seq)
        }
        Value::Mapping(map) => {
            let mut yaml_map = serde_yaml::Mapping::new();
            for (key, value) in map {
                yaml_map.insert(serde_yaml::Value::String(key.clone()), to_yaml(value));
            }
            serde_yaml::Value::Mapping(yaml_map)
        }
    }
}

impl YamlWriter {
    pub fn new_boxed() -> Box<Self> {
        Box::new(Self{})
    }
}
