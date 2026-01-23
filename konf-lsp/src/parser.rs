//! Parser module for konf YAML files
//!
//! Handles parsing YAML with konf-specific metadata (`<!>` section)
//! and template references (`${path.to.value}`).

use std::collections::HashMap;
use std::sync::OnceLock;

use regex::Regex;
use serde_yaml::Value as YamlValue;

// Re-use utilities from the base lib
pub use konf_provider::imports::{resolve_relative_path, METADATA_KEY};

/// Regex for template references: ${path.to.value}
static TEMPLATE_RE: OnceLock<Regex> = OnceLock::new();
/// Regex for incomplete template references (for completion): ${path.to.value (no closing brace)
static INCOMPLETE_TEMPLATE_RE: OnceLock<Regex> = OnceLock::new();

fn template_re() -> &'static Regex {
    TEMPLATE_RE.get_or_init(|| Regex::new(r"\$\{(?P<path>[^}]+)\}").expect("invalid regex"))
}

fn incomplete_template_re() -> &'static Regex {
    // Matches ${... without closing brace (for autocompletion while typing)
    INCOMPLETE_TEMPLATE_RE.get_or_init(|| Regex::new(r"\$\{(?P<path>[^}]*)$").expect("invalid regex"))
}

/// Represents an import with its path and alias
#[derive(Debug, Clone)]
pub struct ImportInfo {
    /// The path to the imported file (can be relative: ../common/db or absolute: common/db)
    pub path: String,
    /// The alias used in templates (e.g., "db" for ${db.host})
    pub alias: String,
    /// The resolved absolute path (after resolving ../ and ./)
    pub resolved_path: Option<String>,
}

/// Represents the metadata section of a konf config file
#[derive(Debug, Clone, Default)]
pub struct KonfMetadata {
    /// Imported config files: alias -> ImportInfo
    pub imports: HashMap<String, ImportInfo>,
    /// List of auth tokens (git mode only)
    #[allow(dead_code)]
    pub auth: Vec<String>,
}

/// A parsed konf config file
#[derive(Debug, Clone)]
pub struct KonfDocument {
    /// The file's relative path (key used for imports)
    pub key: String,
    /// Raw YAML content
    pub content: String,
    /// Parsed YAML value
    pub yaml: Option<YamlValue>,
    /// Extracted metadata
    pub metadata: KonfMetadata,
    /// All template references found in the document
    pub template_refs: Vec<TemplateRef>,
    /// Top-level keys (excluding metadata)
    #[allow(dead_code)]
    pub keys: Vec<String>,
}

/// A template reference found in the document
#[derive(Debug, Clone)]
pub struct TemplateRef {
    /// The full reference path (e.g., "common/database.host")
    pub path: String,
    /// Line number (0-indexed)
    pub line: usize,
    /// Column start (0-indexed)
    pub col_start: usize,
    /// Column end (0-indexed)
    pub col_end: usize,
}

impl KonfDocument {
    /// Parse a konf YAML document
    pub fn parse(key: String, content: String) -> Self {
        let yaml = serde_yaml::from_str::<YamlValue>(&content).ok();
        let metadata = yaml
            .as_ref()
            .map(|y| extract_metadata(y, &key))
            .unwrap_or_default();
        let template_refs = find_template_refs(&content);
        let keys = yaml
            .as_ref()
            .map(extract_top_level_keys)
            .unwrap_or_default();

        tracing::debug!(
            "Parsed document '{}': {} imports, {} template refs",
            key,
            metadata.imports.len(),
            template_refs.len()
        );
        for (alias, info) in &metadata.imports {
            tracing::debug!(
                "  Import: alias='{}', path='{}', resolved='{:?}'",
                alias,
                info.path,
                info.resolved_path
            );
        }

        Self {
            key,
            content,
            yaml,
            metadata,
            template_refs,
            keys,
        }
    }

    /// Get all available keys at a given path in this document
    pub fn get_keys_at_path(&self, path: &[&str]) -> Vec<KeyInfo> {
        let Some(yaml) = &self.yaml else {
            return vec![];
        };

        let mut current = yaml;

        // Navigate to the path
        for part in path {
            match current {
                YamlValue::Mapping(map) => {
                    if let Some(val) = map.get(YamlValue::String(part.to_string())) {
                        current = val;
                    } else {
                        return vec![];
                    }
                }
                _ => return vec![],
            }
        }

        // Extract keys from current position
        match current {
            YamlValue::Mapping(map) => map
                .iter()
                .filter_map(|(k, v)| {
                    let key = k.as_str()?;
                    // Skip metadata key
                    if key == METADATA_KEY {
                        return None;
                    }
                    Some(KeyInfo {
                        name: key.to_string(),
                        value_type: value_type_name(v),
                        preview: value_preview(v),
                    })
                })
                .collect(),
            _ => vec![],
        }
    }

    /// Get the value at a given path
    pub fn get_value_at_path(&self, path: &[&str]) -> Option<&YamlValue> {
        let yaml = self.yaml.as_ref()?;
        let mut current = yaml;

        for part in path {
            match current {
                YamlValue::Mapping(map) => {
                    current = map.get(YamlValue::String(part.to_string()))?;
                }
                _ => return None,
            }
        }

        Some(current)
    }

    /// Find the line and column where a key path is defined
    /// Returns (line, column) where line is 0-indexed
    pub fn find_key_position(&self, path: &[&str]) -> Option<(u32, u32)> {
        if path.is_empty() {
            return Some((0, 0));
        }

        let lines: Vec<&str> = self.content.lines().collect();
        let mut current_indent = 0i32;
        let mut path_index = 0;

        for (line_idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            // Calculate indentation (spaces)
            let indent = line.len() - trimmed.len();

            // Check if this line defines the key we're looking for
            let target_key = path[path_index];
            let key_pattern = format!("{}:", target_key);

            if trimmed.starts_with(&key_pattern) {
                // Check if indentation matches expected level
                let expected_indent = if path_index == 0 { 0 } else { current_indent + 2 };

                if (indent as i32) >= expected_indent - 1 && (indent as i32) <= expected_indent + 1 {
                    path_index += 1;
                    current_indent = indent as i32;

                    if path_index == path.len() {
                        // Found the final key
                        let col = line.find(&key_pattern).unwrap_or(0);
                        return Some((line_idx as u32, col as u32));
                    }
                }
            }
        }

        // If we found some of the path but not all, return the last found position
        None
    }
}

/// Information about a key in the config
#[derive(Debug, Clone)]
pub struct KeyInfo {
    pub name: String,
    pub value_type: String,
    pub preview: String,
}

/// Extract metadata from a konf YAML document
/// `doc_key` is the key of the current document, used for resolving relative paths
fn extract_metadata(yaml: &YamlValue, doc_key: &str) -> KonfMetadata {
    let Some(mapping) = yaml.as_mapping() else {
        return KonfMetadata::default();
    };

    let Some(meta) = mapping.get(YamlValue::String(METADATA_KEY.to_string())) else {
        return KonfMetadata::default();
    };

    let Some(meta_map) = meta.as_mapping() else {
        return KonfMetadata::default();
    };

    let mut imports = HashMap::new();

    // Parse imports - mapping of path -> alias
    // If alias is null or empty, the path is used as the alias
    if let Some(import_value) = meta_map.get(YamlValue::String("import".to_string())) {
        if let YamlValue::Mapping(map) = import_value {
            for (path_val, alias_val) in map {
                if let Some(path) = path_val.as_str() {
                    // If alias is a non-empty string, use it; otherwise use the path as alias
                    let alias = match alias_val.as_str() {
                        Some(s) if !s.is_empty() => s.to_string(),
                        _ => path.to_string(), // Null, empty string, or other → use path as alias
                    };
                    let resolved = resolve_relative_path(doc_key, path);
                    imports.insert(
                        alias.clone(),
                        ImportInfo {
                            path: path.to_string(),
                            alias,
                            resolved_path: Some(resolved),
                        },
                    );
                }
            }
        }
    }

    let auth = meta_map
        .get(YamlValue::String("auth".to_string()))
        .and_then(|v| v.as_sequence())
        .map(|seq| {
            seq.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    KonfMetadata { imports, auth }
}

/// Extract top-level keys from a YAML document (excluding metadata)
fn extract_top_level_keys(yaml: &YamlValue) -> Vec<String> {
    let Some(mapping) = yaml.as_mapping() else {
        return vec![];
    };

    mapping
        .keys()
        .filter_map(|k| {
            let key = k.as_str()?;
            if key == METADATA_KEY {
                None
            } else {
                Some(key.to_string())
            }
        })
        .collect()
}

/// Find all template references in the content
fn find_template_refs(content: &str) -> Vec<TemplateRef> {
    let mut refs = vec![];

    for (line_idx, line) in content.lines().enumerate() {
        for cap in template_re().captures_iter(line) {
            if let Some(path_match) = cap.name("path") {
                let full_match = cap.get(0).unwrap();
                refs.push(TemplateRef {
                    path: path_match.as_str().to_string(),
                    line: line_idx,
                    col_start: full_match.start(),
                    col_end: full_match.end(),
                });
            }
        }
    }

    refs
}

/// Get a human-readable type name for a YAML value
fn value_type_name(value: &YamlValue) -> String {
    match value {
        YamlValue::String(_) => "String".to_string(),
        YamlValue::Number(n) => {
            if n.is_i64() {
                "Int".to_string()
            } else {
                "Float".to_string()
            }
        }
        YamlValue::Bool(_) => "Boolean".to_string(),
        YamlValue::Sequence(_) => "Sequence".to_string(),
        YamlValue::Mapping(_) => "Mapping".to_string(),
        YamlValue::Null => "Null".to_string(),
        YamlValue::Tagged(_) => "Tagged".to_string(),
    }
}

/// Get a preview of a YAML value
fn value_preview(value: &YamlValue) -> String {
    match value {
        YamlValue::String(s) => {
            if s.len() > 50 {
                format!("\"{}...\"", &s[..47])
            } else {
                format!("\"{s}\"")
            }
        }
        YamlValue::Number(n) => n.to_string(),
        YamlValue::Bool(b) => b.to_string(),
        YamlValue::Sequence(seq) => format!("[{} items]", seq.len()),
        YamlValue::Mapping(map) => format!("{{{} keys}}", map.len()),
        YamlValue::Null => "null".to_string(),
        YamlValue::Tagged(t) => format!("!{} ...", t.tag),
    }
}

/// Parse a template reference path into (file_key, key_path)
/// e.g., "common/database.host.port" -> ("common/database", ["host", "port"])
pub fn parse_template_path(path: &str) -> Option<(String, Vec<String>)> {
    let mut parts: Vec<&str> = path.split('.').collect();

    if parts.is_empty() {
        return None;
    }

    let file_key = parts.remove(0).to_string();
    let key_path: Vec<String> = parts.iter().map(|s| s.to_string()).collect();

    Some((file_key, key_path))
}

/// Check if a position is inside a template reference
pub fn get_template_at_position(content: &str, line: usize, col: usize) -> Option<TemplateContext> {
    let line_content = content.lines().nth(line)?;

    // First check complete templates: ${path}
    for cap in template_re().captures_iter(line_content) {
        let full_match = cap.get(0)?;
        if col >= full_match.start() && col <= full_match.end() {
            let path = cap.name("path")?.as_str();
            let path_start = full_match.start() + 2; // after "${"
            let cursor_in_path = col.saturating_sub(path_start);

            return Some(TemplateContext {
                full_path: path.to_string(),
                cursor_offset: cursor_in_path,
            });
        }
    }

    // Then check incomplete templates (for autocompletion): ${path (no closing brace)
    // Only check the portion of the line up to the cursor
    let before_cursor = &line_content[..col.min(line_content.len())];
    if let Some(cap) = incomplete_template_re().captures(before_cursor) {
        let full_match = cap.get(0)?;
        let path = cap.name("path")?.as_str();
        let path_start = full_match.start() + 2; // after "${"
        let cursor_in_path = col.saturating_sub(path_start);

        return Some(TemplateContext {
            full_path: path.to_string(),
            cursor_offset: cursor_in_path,
        });
    }

    None
}

/// Context about the cursor position within a template
#[derive(Debug)]
pub struct TemplateContext {
    /// The full path inside the template
    pub full_path: String,
    /// Cursor offset within the path
    pub cursor_offset: usize,
}

impl TemplateContext {
    /// Get the portion of the path before the cursor
    pub fn path_before_cursor(&self) -> &str {
        let end = self.cursor_offset.min(self.full_path.len());
        &self.full_path[..end]
    }

    /// Parse the path to determine completion context
    pub fn completion_context(&self) -> CompletionContext {
        let before = self.path_before_cursor();

        if !before.contains('.') {
            // Still typing the file name
            CompletionContext::FileName {
                partial: before.to_string(),
            }
        } else {
            // Typing a key path
            let parts: Vec<&str> = before.split('.').collect();
            let file_key = parts[0].to_string();
            let key_path: Vec<String> = parts[1..parts.len() - 1]
                .iter()
                .map(|s| s.to_string())
                .collect();
            let partial = parts.last().map(|s| s.to_string()).unwrap_or_default();

            CompletionContext::KeyPath {
                file_key,
                key_path,
                partial,
            }
        }
    }
}

/// Context for completion
#[derive(Debug)]
pub enum CompletionContext {
    /// Completing a file name (before the first dot)
    FileName { partial: String },
    /// Completing a key path (after at least one dot)
    KeyPath {
        file_key: String,
        key_path: Vec<String>,
        partial: String,
    },
}

/// Check if position is in the import section
pub fn is_in_import_section(content: &str, line: usize) -> bool {
    let lines: Vec<&str> = content.lines().collect();

    // Check if we're in the <!>: section
    let mut in_metadata = false;
    let mut in_import = false;

    for (idx, l) in lines.iter().enumerate() {
        let trimmed = l.trim();

        if trimmed.starts_with("<!>:") {
            in_metadata = true;
            continue;
        }

        if in_metadata {
            // Check for end of metadata section (non-indented line that's not empty)
            if !l.starts_with(' ') && !l.starts_with('\t') && !trimmed.is_empty() && !trimmed.starts_with('-') {
                in_metadata = false;
                in_import = false;
            }

            if trimmed.starts_with("import:") {
                in_import = true;
            } else if trimmed.starts_with("auth:") {
                in_import = false;
            }
        }

        if idx == line {
            return in_metadata && in_import;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_template_path() {
        let (file, keys) = parse_template_path("common/database.host").unwrap();
        assert_eq!(file, "common/database");
        assert_eq!(keys, vec!["host"]);

        let (file, keys) = parse_template_path("db.nested.key.path").unwrap();
        assert_eq!(file, "db");
        assert_eq!(keys, vec!["nested", "key", "path"]);
    }

    #[test]
    fn test_is_in_import_section() {
        let content = r#"<!>:
  import:
    common/database:
    common/redis: cache

service:
  name: test
"#;
        assert!(is_in_import_section(content, 2));
        assert!(is_in_import_section(content, 3));
        assert!(!is_in_import_section(content, 5));
        assert!(!is_in_import_section(content, 6));
    }

    #[test]
    fn test_find_key_position() {
        let content = r#"host: localhost
port: 5432
database:
  name: mydb
  user: admin
  nested:
    deep: value
"#;
        let doc = KonfDocument::parse("test".to_string(), content.to_string());

        // Top-level key
        let pos = doc.find_key_position(&["host"]);
        assert_eq!(pos, Some((0, 0)));

        let pos = doc.find_key_position(&["port"]);
        assert_eq!(pos, Some((1, 0)));

        // Nested key
        let pos = doc.find_key_position(&["database", "name"]);
        assert_eq!(pos, Some((3, 2)));

        let pos = doc.find_key_position(&["database", "user"]);
        assert_eq!(pos, Some((4, 2)));

        // Deep nested key
        let pos = doc.find_key_position(&["database", "nested", "deep"]);
        assert_eq!(pos, Some((6, 4)));

        // Non-existent key
        let pos = doc.find_key_position(&["nonexistent"]);
        assert_eq!(pos, None);

        // Empty path returns (0, 0)
        let pos = doc.find_key_position(&[]);
        assert_eq!(pos, Some((0, 0)));
    }
}
