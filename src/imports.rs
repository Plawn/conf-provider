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
use serde_yaml::Value as YamlValue;

/// The metadata key used in konf config files
pub const METADATA_KEY: &str = "<!>";

/// Information about an import declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportInfo {
    /// The original path as written in the config file (can be relative: ../common/db)
    pub path: String,
    /// The alias used in templates (e.g., "db" for ${db.host})
    pub alias: String,
    /// The resolved absolute path (after resolving ../ and ./).
    /// This is `Some` when the path has been resolved against a document key,
    /// or `None` when path resolution is not needed (e.g., in LSP context).
    pub resolved_path: Option<String>,
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
                    resolved_path: Some(resolved),
                },
            );
        }
    }

    imports
}

/// Parse imports from a serde_yaml::Value.
///
/// This function is useful when working directly with serde_yaml (e.g., in the LSP)
/// rather than the internal `Value` type.
///
/// # Arguments
/// * `yaml` - The YAML value containing the `<!>` metadata section
/// * `doc_key` - Optional key of the current document, used for resolving relative paths.
///   If `None`, `resolved_path` in the returned `ImportInfo` will be `None`.
///
/// # Returns
/// A HashMap mapping alias to ImportInfo
pub fn parse_imports_from_yaml(
    yaml: &YamlValue,
    doc_key: Option<&str>,
) -> HashMap<String, ImportInfo> {
    let Some(mapping) = yaml.as_mapping() else {
        return HashMap::new();
    };

    let Some(meta) = mapping.get(YamlValue::String(METADATA_KEY.to_string())) else {
        return HashMap::new();
    };

    let Some(meta_map) = meta.as_mapping() else {
        return HashMap::new();
    };

    let Some(import_value) = meta_map.get(YamlValue::String("import".to_string())) else {
        return HashMap::new();
    };

    let Some(import_map) = import_value.as_mapping() else {
        return HashMap::new();
    };

    let mut imports = HashMap::new();

    for (path_val, alias_val) in import_map {
        let Some(path) = path_val.as_str() else {
            continue;
        };

        // If alias is a non-empty string, use it; otherwise use the path as alias
        let alias = match alias_val.as_str() {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => path.to_string(), // Null, empty string, or other → use path as alias
        };

        let resolved_path = doc_key.map(|key| resolve_relative_path(key, path));

        imports.insert(
            alias.clone(),
            ImportInfo {
                path: path.to_string(),
                alias,
                resolved_path,
            },
        );
    }

    imports
}

/// Get import paths as a list (for backwards compatibility).
///
/// Returns the resolved paths for all imports.
pub fn get_import_paths(value: &Value, doc_key: &str) -> Vec<String> {
    parse_imports(value, doc_key)
        .values()
        .filter_map(|info| info.resolved_path.clone())
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
        assert_eq!(db_import.resolved_path, Some("common/database".to_string()));

        let redis_import = imports.get("common/redis").unwrap();
        assert_eq!(redis_import.path, "common/redis");
        assert_eq!(redis_import.alias, "common/redis");
        assert_eq!(redis_import.resolved_path, Some("common/redis".to_string()));
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
        assert_eq!(db_import.resolved_path, Some("common/database".to_string()));

        let cache_import = imports.get("cache").unwrap();
        assert_eq!(cache_import.path, "common/redis");
        assert_eq!(cache_import.alias, "cache");
        assert_eq!(cache_import.resolved_path, Some("common/redis".to_string()));
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
        assert_eq!(db_import.resolved_path, Some("common/database".to_string()));

        let cfg_import = imports.get("cfg").unwrap();
        assert_eq!(cfg_import.path, "./config");
        assert_eq!(cfg_import.resolved_path, Some("services/config".to_string()));
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

    #[test]
    fn test_parse_imports_from_yaml_with_doc_key() {
        let yaml: YamlValue = serde_yaml::from_str(
            r#"
<!>:
  import:
    common/database: db
    ../shared/redis: cache
service:
  name: test
"#,
        )
        .unwrap();

        let imports = parse_imports_from_yaml(&yaml, Some("services/api"));

        assert_eq!(imports.len(), 2);

        let db_import = imports.get("db").unwrap();
        assert_eq!(db_import.path, "common/database");
        assert_eq!(db_import.alias, "db");
        assert_eq!(db_import.resolved_path, Some("common/database".to_string()));

        let cache_import = imports.get("cache").unwrap();
        assert_eq!(cache_import.path, "../shared/redis");
        assert_eq!(cache_import.alias, "cache");
        assert_eq!(cache_import.resolved_path, Some("shared/redis".to_string()));
    }

    #[test]
    fn test_parse_imports_from_yaml_without_doc_key() {
        let yaml: YamlValue = serde_yaml::from_str(
            r#"
<!>:
  import:
    common/database: db
    common/redis:
"#,
        )
        .unwrap();

        let imports = parse_imports_from_yaml(&yaml, None);

        assert_eq!(imports.len(), 2);

        let db_import = imports.get("db").unwrap();
        assert_eq!(db_import.path, "common/database");
        assert_eq!(db_import.alias, "db");
        assert_eq!(db_import.resolved_path, None);

        // Null alias uses path as alias
        let redis_import = imports.get("common/redis").unwrap();
        assert_eq!(redis_import.path, "common/redis");
        assert_eq!(redis_import.alias, "common/redis");
        assert_eq!(redis_import.resolved_path, None);
    }

    #[test]
    fn test_parse_imports_from_yaml_empty() {
        let yaml: YamlValue = serde_yaml::from_str("service: test").unwrap();
        let imports = parse_imports_from_yaml(&yaml, Some("test"));
        assert!(imports.is_empty());
    }
}
