//! Import parsing module for konf configuration files.
//!
//! Handles parsing of import declarations in the `<!>` metadata section.
//!
//! # Import Format
//!
//! Imports use a key-value mapping where:
//! - The key is the path to the imported file (can be relative: ../common/db)
//! - The value is the alias used in templates (e.g., "db" for ${db.host})
//! - If the value is empty/null, the path is used as the alias
//!
//! ```yaml
//! <!>:
//!   import:
//!     common/database:          # Null value → uses "common/database" as alias
//!     common/redis: cache       # Explicit alias "cache"
//!     ../shared/config: cfg     # Relative path with alias
//! ```

use std::collections::HashMap;

use crate::Value;

/// The metadata key used in konf config files
pub const METADATA_KEY: &str = "<!>";

/// Information about an import declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportInfo {
    /// The original path as written in the config file (can be relative: ../common/db)
    pub path: String,
    /// The alias used in templates (e.g., "db" for ${db.host})
    pub alias: String,
    /// The resolved absolute path (after resolving ../ and ./)
    pub resolved_path: String,
}

/// Parse imports from a configuration value.
///
/// The import section should be a mapping of path to alias:
/// ```yaml
/// import:
///   common/database: db      # path -> alias
///   common/redis:            # path with null value -> uses path as alias
/// ```
///
/// # Arguments
/// * `value` - The configuration value containing the `<!>` metadata section
/// * `doc_key` - The key of the current document, used for resolving relative paths
///
/// # Returns
/// A HashMap mapping alias to ImportInfo
pub fn parse_imports(value: &Value, doc_key: &str) -> HashMap<String, ImportInfo> {
    let Some(main_value) = value.get(METADATA_KEY) else {
        return HashMap::new();
    };

    let Some(main_map) = main_value.as_mapping() else {
        return HashMap::new();
    };

    let Some(import_value) = main_map.get("import") else {
        return HashMap::new();
    };

    let mut imports = HashMap::new();

    // Import section should be a mapping of path -> alias
    // If alias is null or empty, the path is used as the alias
    if let Value::Mapping(map) = import_value {
        for (path_key, alias_value) in map {
            // If alias is a non-empty string, use it; otherwise use the path as alias
            let alias = match alias_value {
                Value::String(s) if !s.is_empty() => s.clone(),
                _ => path_key.clone(), // Null, empty string, or other → use path as alias
            };
            let resolved = resolve_relative_path(doc_key, path_key);
            imports.insert(
                alias.clone(),
                ImportInfo {
                    path: path_key.clone(),
                    alias,
                    resolved_path: resolved,
                },
            );
        }
    }

    imports
}

/// Get import paths as a list (for backwards compatibility).
///
/// Returns the resolved paths for all imports.
pub fn get_import_paths(value: &Value, doc_key: &str) -> Vec<String> {
    parse_imports(value, doc_key)
        .values()
        .map(|info| info.resolved_path.clone())
        .collect()
}

/// Resolve a relative path (../, ./) against the current document's directory.
///
/// # Examples
/// ```
/// use konf_provider::imports::resolve_relative_path;
///
/// // Relative path from services/api
/// assert_eq!(
///     resolve_relative_path("services/api", "../common/database"),
///     "common/database"
/// );
///
/// // Absolute path (no change)
/// assert_eq!(
///     resolve_relative_path("services/api", "common/database"),
///     "common/database"
/// );
///
/// // Current directory reference
/// assert_eq!(
///     resolve_relative_path("services/api", "./config"),
///     "services/config"
/// );
/// ```
pub fn resolve_relative_path(doc_key: &str, import_path: &str) -> String {
    // If not a relative path, return as-is
    if !import_path.starts_with("../") && !import_path.starts_with("./") {
        return import_path.to_string();
    }

    // Get the directory of the current document
    let doc_dir = if let Some(pos) = doc_key.rfind('/') {
        &doc_key[..pos]
    } else {
        ""
    };

    // Split into path components
    let mut components: Vec<&str> = if doc_dir.is_empty() {
        vec![]
    } else {
        doc_dir.split('/').collect()
    };

    // Process the import path
    for part in import_path.split('/') {
        match part {
            ".." => {
                components.pop();
            }
            "." | "" => {}
            _ => {
                components.push(part);
            }
        }
    }

    components.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Mapping;

    fn make_mapping(entries: Vec<(&str, Value)>) -> Mapping {
        entries
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect()
    }

    #[test]
    fn test_resolve_relative_path_no_change() {
        assert_eq!(
            resolve_relative_path("services/api", "common/database"),
            "common/database"
        );
    }

    #[test]
    fn test_resolve_relative_path_parent() {
        assert_eq!(
            resolve_relative_path("services/api", "../common/database"),
            "common/database"
        );
    }

    #[test]
    fn test_resolve_relative_path_current() {
        assert_eq!(
            resolve_relative_path("services/api", "./config"),
            "services/config"
        );
    }

    #[test]
    fn test_resolve_relative_path_multiple_parents() {
        assert_eq!(
            resolve_relative_path("services/api/v1", "../../common/database"),
            "common/database"
        );
    }

    #[test]
    fn test_resolve_relative_path_from_root() {
        assert_eq!(
            resolve_relative_path("api", "../common/database"),
            "common/database"
        );
    }

    #[test]
    fn test_parse_imports_null_alias() {
        // When alias value is null, the path should be used as the alias
        let value = Value::Mapping(make_mapping(vec![(
            "<!>",
            Value::Mapping(make_mapping(vec![(
                "import",
                Value::Mapping(make_mapping(vec![
                    ("common/database", Value::Null),
                    ("common/redis", Value::Null),
                ])),
            )])),
        )]));

        let imports = parse_imports(&value, "services/api");

        assert_eq!(imports.len(), 2);

        let db_import = imports.get("common/database").unwrap();
        assert_eq!(db_import.path, "common/database");
        assert_eq!(db_import.alias, "common/database");
        assert_eq!(db_import.resolved_path, "common/database");

        let redis_import = imports.get("common/redis").unwrap();
        assert_eq!(redis_import.path, "common/redis");
        assert_eq!(redis_import.alias, "common/redis");
        assert_eq!(redis_import.resolved_path, "common/redis");
    }

    #[test]
    fn test_parse_imports_new_format() {
        let value = Value::Mapping(make_mapping(vec![(
            "<!>",
            Value::Mapping(make_mapping(vec![(
                "import",
                Value::Mapping(make_mapping(vec![
                    ("common/database", Value::String("db".to_string())),
                    ("common/redis", Value::String("cache".to_string())),
                ])),
            )])),
        )]));

        let imports = parse_imports(&value, "services/api");

        assert_eq!(imports.len(), 2);

        let db_import = imports.get("db").unwrap();
        assert_eq!(db_import.path, "common/database");
        assert_eq!(db_import.alias, "db");
        assert_eq!(db_import.resolved_path, "common/database");

        let cache_import = imports.get("cache").unwrap();
        assert_eq!(cache_import.path, "common/redis");
        assert_eq!(cache_import.alias, "cache");
        assert_eq!(cache_import.resolved_path, "common/redis");
    }

    #[test]
    fn test_parse_imports_new_format_with_relative_paths() {
        let value = Value::Mapping(make_mapping(vec![(
            "<!>",
            Value::Mapping(make_mapping(vec![(
                "import",
                Value::Mapping(make_mapping(vec![
                    ("../common/database", Value::String("db".to_string())),
                    ("./config", Value::String("cfg".to_string())),
                ])),
            )])),
        )]));

        let imports = parse_imports(&value, "services/api");

        assert_eq!(imports.len(), 2);

        let db_import = imports.get("db").unwrap();
        assert_eq!(db_import.path, "../common/database");
        assert_eq!(db_import.resolved_path, "common/database");

        let cfg_import = imports.get("cfg").unwrap();
        assert_eq!(cfg_import.path, "./config");
        assert_eq!(cfg_import.resolved_path, "services/config");
    }

    #[test]
    fn test_parse_imports_empty() {
        let value = Value::Mapping(make_mapping(vec![]));
        let imports = parse_imports(&value, "services/api");
        assert!(imports.is_empty());
    }

    #[test]
    fn test_parse_imports_no_metadata() {
        let value = Value::Mapping(make_mapping(vec![(
            "service",
            Value::String("api".to_string()),
        )]));
        let imports = parse_imports(&value, "services/api");
        assert!(imports.is_empty());
    }

    #[test]
    fn test_get_import_paths() {
        let value = Value::Mapping(make_mapping(vec![(
            "<!>",
            Value::Mapping(make_mapping(vec![(
                "import",
                Value::Mapping(make_mapping(vec![
                    ("../common/database", Value::String("db".to_string())),
                    ("common/redis", Value::String("cache".to_string())),
                ])),
            )])),
        )]));

        let paths = get_import_paths(&value, "services/api");

        assert_eq!(paths.len(), 2);
        assert!(paths.contains(&"common/database".to_string()));
        assert!(paths.contains(&"common/redis".to_string()));
    }
}
