use crate::{writer::ValueWriter, Value};

#[derive(Debug)]
pub struct DockerEnvVarWriter {}

impl ValueWriter for DockerEnvVarWriter {
    fn ext(&self) -> &'static str {
        "docker-env"
    }

    fn to_str(&self, v: &Value) -> String {
        let mut lines = Vec::new();
        flatten_to_env("", v, &mut lines);
        lines.join("\n")
    }
}

/// Recursively traverses the Value structure to flatten it into environment variable format.
fn flatten_to_env(prefix: &str, value: &Value, lines: &mut Vec<String>) {
    match value {
        Value::Mapping(map) => {
            for (key, val) in map {
                // Create the new prefix for the next level of recursion
                let new_prefix = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{}_{}", prefix, key)
                };
                flatten_to_env(&new_prefix, val, lines);
            }
        }
        Value::Sequence(seq) => {
            for (index, item) in seq.iter().enumerate() {
                // Append the index to the prefix for sequence items
                let new_prefix = format!("{}_{}", prefix, index);
                flatten_to_env(&new_prefix, item, lines);
            }
        }
        // Base cases for the recursion: primitive values
        Value::String(s) => {
            lines.push(format!("{}={}", prefix.to_uppercase(), s));
        }
        Value::Number(n) => {
            lines.push(format!("{}={}", prefix.to_uppercase(), n));
        }
        Value::Boolean(b) => {
            lines.push(format!("{}={}", prefix.to_uppercase(), b));
        }
        Value::Null => {
            // Represent null as an empty string
            lines.push(format!("{}=\"\"", prefix.to_uppercase()));
        }
    }
}

impl DockerEnvVarWriter {
    pub fn new_boxed() -> Box<Self> {
        Box::new(Self{})
    }
}
