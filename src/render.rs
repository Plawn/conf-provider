use std::collections::HashMap;

use crate::{loader::Value, Konf};


/// Lookup a dotted path like "country.city" inside a nested JSON object.
pub fn lookup_path<'a>(dag: &'a HashMap<String, Konf>, path: &str) -> Option<Value> {
    let mut s = path.split('.');
    let file_key = s.next().expect("not root key");
    let k = dag.get(file_key).expect("missing data");
    let root = k.rendered.clone().expect("not rendered");
    let mut current = root;
    for key in s {
        match current {
            Value::Mapping(map) => {
                current = map.get(key).cloned()?;
            }
            _ => return None,
        }
    }
    Some(current)
}
/// Traverse a JSON `Value` and replace any `"#ref:<path>"` with the corresponding value in `map`.
pub fn resolve_refs(value: &mut Value, map: &HashMap<String, Konf>) {
    match value {
        Value::String(s) => {
            if let Some(path) = s.strip_prefix("#ref:") {
                if let Some(replacement) = lookup_path(map, path) {
                    *value = replacement.clone();
                }
            }
        }
        Value::Sequence(arr) => {
            for v in arr {
                resolve_refs(v, map);
            }
        }
        Value::Mapping(obj) => {
            for (_k, v) in obj.iter_mut() {
                resolve_refs(v, map);
            }
        }
        _ => {},
    }
}