use crate::{Value, writer::{ValueWriter, WriterError}};

#[derive(Debug)]
pub struct PropertiesWriter {}

impl ValueWriter for PropertiesWriter {
    fn ext(&self) -> &'static str {
        "properties"
    }

    fn to_str(&self, v: &Value) -> Result<String, WriterError> {
        let mut properties = String::new();
        write_properties(v, "", &mut properties);
        Ok(properties)
    }
}

fn write_properties(value: &Value, prefix: &str, properties: &mut String) {
    match value {
        Value::Mapping(map) => {
            for (key, val) in map {
                let new_prefix = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{}.{}", prefix, key)
                };
                write_properties(val, &new_prefix, properties);
            }
        }
        Value::Sequence(seq) => {
            for (index, val) in seq.iter().enumerate() {
                let new_prefix = format!("{}[{}]", prefix, index);
                write_properties(val, &new_prefix, properties);
            }
        }
        Value::String(s) => {
            properties.push_str(&format!("{}=\"{}\"\n", prefix, s));
        }
        Value::Int(n) => {
            properties.push_str(&format!("{}={}\n", prefix, n));
        }
        Value::Float(n) => {
            properties.push_str(&format!("{}={}\n", prefix, n));
        }
        Value::Boolean(b) => {
            properties.push_str(&format!("{}={}\n", prefix, b));
        }
        Value::Null => {
            // Java properties files don't have a concept of null,
            // so we can either ignore it or write an empty string.
            // Here, we'll write an empty string.
            properties.push_str(&format!("{}=\n", prefix));
        }
    }
}

impl PropertiesWriter {
    pub fn new_boxed() -> Box<Self> {
        Box::new(Self {})
    }
}
