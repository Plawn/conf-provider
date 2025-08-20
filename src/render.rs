use std::sync::Arc;

use crate::loader::Value;
use crate::{DagFiles, RenderCache};

// --- Assuming Konf, DagFiles, RenderCache, etc. are defined as in the previous answer ---

/// NEW: Looks up a path by checking the local cache first, then the global snapshot.
/// Returns a reference to avoid unnecessary clones.
fn lookup_path_with_cache<'a>(
    path: &str,
    local_cache: &'a RenderCache,
    global_snapshot: &'a Arc<DagFiles>,
) -> Option<&'a Value> {
    let mut parts = path.split('.');
    // The first part of the path is the key to the file map
    let file_key = parts.next()?;

    // --- The Core Logic Change is Here ---
    // First, find the root rendered value for the file.
    // Priority 1: Check the local cache.
    let root_value = local_cache.get(file_key).or_else(|| {
        // Priority 2: Check the global snapshot's rendered field.
        global_snapshot
            .get(file_key)
            .and_then(|k| k.rendered.as_ref())
    })?;
    // --- End of Core Logic Change ---

    // Now, traverse the rest of the path within the Value, just like before.
    let mut current = root_value;
    for key in parts {
        match current {
            Value::Mapping(map) => {
                current = map.get(key)?; // Use `?` for graceful failure
            }
            _ => return None,
        }
    }
    Some(current)
}

/// NEW: Traverses a Value and resolves all "#ref:" strings using the two-tiered cache.
pub fn resolve_refs_with_cache(
    value: &mut Value,
    local_cache: &RenderCache,
    global_snapshot: &Arc<DagFiles>,
) {
    match value {
        Value::String(s) => {
            if let Some(path) = s.strip_prefix("#ref:") {
                // Use our new lookup function
                if let Some(replacement) =
                    lookup_path_with_cache(path, local_cache, global_snapshot)
                {
                    // Clone only when we're actually replacing the value.
                    *value = replacement.clone();
                }
                // Optional: You might want to log an error or handle cases
                // where a ref can't be found.
            }
        }
        Value::Sequence(arr) => {
            for v in arr {
                // Recurse, passing the caches down
                resolve_refs_with_cache(v, local_cache, global_snapshot);
            }
        }
        Value::Mapping(obj) => {
            for (_k, v) in obj.iter_mut() {
                // Recurse, passing the caches down
                resolve_refs_with_cache(v, local_cache, global_snapshot);
            }
        }
        _ => {}
    }
}
