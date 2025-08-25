use std::collections::HashMap;

use crate::Value;

use lazy_static::lazy_static;
use regex::{Captures, Regex};

lazy_static! {
    /// Regex for an exact match, e.g., "${a.b.c}"
    static ref EXACT_MATCH_RE: Regex = Regex::new(r"^\$\{(?P<path>[^}]+)\}$").unwrap();
    /// Regex for finding all occurrences, e.g., in "http://${host}/${path}"
    static ref INTERPOLATION_RE: Regex = Regex::new(r"\$\{(?P<path>[^}]+)\}").unwrap();
}

/// Helper to look up a dotted path (e.g., "dependency_file.some.nested.key")
/// within the pre-rendered dependencies map.
/// (This function remains unchanged from your original code).
fn lookup_in_deps<'a>(path: &str, deps: &'a HashMap<String, Value>) -> Option<&'a Value> {
    let mut parts = path.split('.');

    // The first part of the path is the key to the top-level dependency map.
    let file_key = parts.next()?;

    // Find the root `Value` for this dependency in our map.
    let mut current = deps.get(file_key)?;

    // Traverse the rest of the path parts to find the nested value.
    for key in parts {
        current = current.get(key)?;
    }

    Some(current)
}

/// Helper to stringify a `serde_yaml::Value` for interpolation.
/// Complex types like Mappings and Sequences return None as they can't be
/// meaningfully embedded in a string.
fn value_to_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        _ => None,
    }
}

/// Traverses a `serde_yaml::Value` and replaces any `"${path}"` strings
/// with the corresponding values found in the `deps` map.
pub fn resolve_refs_from_deps(value: &mut Value, deps: &HashMap<String, Value>) {
    match value {
        Value::String(s) => {
            // Case 1: The entire string is a single placeholder, like "${a.b.c}".
            // In this case, we replace the string with the referenced value, preserving its type.
            if let Some(caps) = EXACT_MATCH_RE.captures(s) {
                if let Some(path) = caps.name("path") {
                    if let Some(replacement) = lookup_in_deps(path.as_str(), deps) {
                        *value = replacement.clone();
                    }
                    // Optional: Log a warning if the reference is not found.
                }
                return; // Stop processing to avoid falling through to interpolation logic.
            }

            // Case 2: The string contains one or more placeholders for interpolation,
            // like "http://${server.host}:${server.port}/path".
            // The result will always be a new string.
            let new_s = INTERPOLATION_RE.replace_all(s, |caps: &Captures| {
                // Get the path from the "path" capture group.
                caps.name("path")
                    .and_then(|path| lookup_in_deps(path.as_str(), deps)) // Look up the value.
                    .and_then(value_to_string) // Convert the `Value` to a `String`, if possible.
                    .unwrap_or_else(|| caps[0].to_string()) // If lookup or conversion fails, leave the placeholder unchanged.
            });

            // If replace_all found and replaced something, it returns an Owned Cow.
            // We update the original value only if a change was made.
            if let std::borrow::Cow::Owned(owned_s) = new_s {
                *value = Value::String(owned_s);
            }
        }
        Value::Sequence(arr) => {
            // Recurse for each item in the sequence.
            for v in arr {
                resolve_refs_from_deps(v, deps);
            }
        }
        Value::Mapping(obj) => {
            // Recurse for each value in the map.
            for (_k, v) in obj.iter_mut() {
                resolve_refs_from_deps(v, deps);
            }
        }
        // Other types (Number, Bool, Null) don't have refs, so we do nothing.
        _ => {}
    }
}

pub fn get_imports(value: &Value) -> Vec<String> {
    const IMPORT_KEY: &str = "import";
    value
        .get(IMPORT_KEY)
        .and_then(|e| e.as_sequence())
        .map(|seq| {
            seq.iter()
                .filter_map(|e| e.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}
