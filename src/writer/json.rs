use crate::{writer::{ValueWriter, WriterError}, Value};

#[derive(Debug)]
pub struct JsonWriter {}

impl ValueWriter for JsonWriter {
    fn ext(&self) -> &'static str {
        "json"
    }
    fn to_str(&self, v: &Value) -> Result<String, WriterError> {
        serde_json::to_string(&to_json(v)).map_err(|e| WriterError {
            format: "json",
            message: e.to_string(),
        })
    }
}

pub fn to_json(value: &Value) -> serde_json::Value {
    use serde_json::{Map, Value as JsonValue};
    match value {
        Value::Number(n) => serde_json::Number::from_f64(*n)
            .map(JsonValue::Number)
            .unwrap_or(JsonValue::Null),
        Value::String(s) => serde_json::Value::String(s.clone()),
        Value::Boolean(b) => serde_json::Value::Bool(*b),
        Value::Null => serde_json::Value::Null,
        Value::Sequence(seq) => {
            let arr: Vec<JsonValue> = seq.iter().map(to_json).collect();
            JsonValue::Array(arr)
        }
        Value::Mapping(map) => {
            let mut obj = Map::new();
            for (key, value) in map {
                // assuming your keys are always strings in `Value::Mapping`
                obj.insert(key.clone(), to_json(value));
            }
            JsonValue::Object(obj)
        }
    }
}

impl JsonWriter {
    pub fn new_boxed() -> Box<Self> {
        Box::new(Self{})
    }
}
