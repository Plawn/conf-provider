use std::collections::HashMap;

use crate::loader::Value;

// --- Assuming Konf, DagFiles, RenderCache, etc. are defined as in the previous answer ---


/// Helper to look up a dotted path (e.g., "dependency_file.some.nested.key")
/// within the pre-rendered dependencies map.
///
/// It returns a reference to avoid cloning until the last possible moment.
fn lookup_in_deps<'a>(path: &str, deps: &'a HashMap<String, Value>) -> Option<&'a Value> {
    let mut parts = path.split('.');

    // The first part of the path is the key to the top-level dependency map.
    let file_key = parts.next()?; // If path is empty, returns None.

    // Find the root `Value` for this dependency in our map.
    let mut current = deps.get(file_key)?;

    // Traverse the rest of the path parts to find the nested value.
    for key in parts {
        // `serde_yaml::Value` has a convenient `.get()` method for mappings.
        current = current.get(key)?;
    }

    Some(current)
}
/// Traverses a `serde_yaml::Value` and replaces any `"#ref:<path>"` strings
/// with the corresponding values found in the `deps` map.
///
/// This is the final version, which operates on a simple map of fully-rendered dependencies.
pub fn resolve_refs_from_deps(value: &mut Value, deps: &HashMap<String, Value>) {
    match value {
        Value::String(s) => {
            if let Some(path) = s.strip_prefix("#ref:") {
                // Use our new, simplified lookup helper.
                if let Some(replacement) = lookup_in_deps(path, deps) {
                    // Clone only when we're actually replacing the string with a Value.
                    *value = replacement.clone();
                }
                // Optional: You could log a warning here if a ref is not found.
                // else { eprintln!("Warning: reference not found for path: {}", path); }
            }
        }
        Value::Sequence(arr) => {
            // Recurse for each item in the array.
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