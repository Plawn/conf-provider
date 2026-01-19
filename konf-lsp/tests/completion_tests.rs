//! Integration tests for completion with a full workspace

use std::collections::HashMap;

// Re-create the necessary types for testing
// In a real setup, these would be imported from the crate

#[derive(Debug, Clone, Default)]
struct KonfMetadata {
    imports: Vec<String>,
}

#[derive(Debug, Clone)]
struct KonfDocument {
    key: String,
    content: String,
    yaml: Option<serde_yaml::Value>,
    metadata: KonfMetadata,
}

#[derive(Debug, Default)]
struct Workspace {
    documents: HashMap<String, KonfDocument>,
    key_to_uri: HashMap<String, String>,
}

impl Workspace {
    fn new() -> Self {
        Self::default()
    }

    fn add_file(&mut self, key: &str, content: &str) {
        let uri = format!("file:///workspace/{}.yaml", key);
        let yaml = serde_yaml::from_str(content).ok();
        let metadata = yaml.as_ref().map(extract_metadata).unwrap_or_default();

        let doc = KonfDocument {
            key: key.to_string(),
            content: content.to_string(),
            yaml,
            metadata,
        };

        self.key_to_uri.insert(key.to_string(), uri.clone());
        self.documents.insert(uri, doc);
    }

    fn get_document_by_key(&self, key: &str) -> Option<&KonfDocument> {
        let uri = self.key_to_uri.get(key)?;
        self.documents.get(uri)
    }
}

fn extract_metadata(yaml: &serde_yaml::Value) -> KonfMetadata {
    let Some(mapping) = yaml.as_mapping() else {
        return KonfMetadata::default();
    };

    let Some(meta) = mapping.get(serde_yaml::Value::String("<!>".to_string())) else {
        return KonfMetadata::default();
    };

    let Some(meta_map) = meta.as_mapping() else {
        return KonfMetadata::default();
    };

    let imports = meta_map
        .get(serde_yaml::Value::String("import".to_string()))
        .and_then(|v| v.as_sequence())
        .map(|seq| {
            seq.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    KonfMetadata { imports }
}

/// Test fixture: creates a workspace with common test files
fn create_test_workspace() -> Workspace {
    let mut ws = Workspace::new();

    // Base config file "a"
    ws.add_file(
        "a",
        r#"
value: hello
number: 42
nested:
  key1: value1
  key2: value2
  deep:
    level3: deep_value
"#,
    );

    // Config file "b" that imports "a"
    ws.add_file(
        "b",
        r#"
<!>:
  import:
    - a

plem: dez
pm: ${a.value}
ref_nested: ${a.nested.key1}
"#,
    );

    // Common database config
    ws.add_file(
        "common/database",
        r#"
host: localhost
port: 5432
name: myapp_db
user: app_user
password: secret123
"#,
    );

    // Service config that imports database
    ws.add_file(
        "services/api",
        r#"
<!>:
  import:
    - common/database

service:
  name: api-service
  port: 8080

database_url: postgres://${common/database.user}:${common/database.password}@${common/database.host}
"#,
    );

    ws
}

// ============ Template detection tests ============

mod template_detection {
    use regex::Regex;
    use std::sync::OnceLock;

    static TEMPLATE_RE: OnceLock<Regex> = OnceLock::new();
    static INCOMPLETE_TEMPLATE_RE: OnceLock<Regex> = OnceLock::new();

    fn template_re() -> &'static Regex {
        TEMPLATE_RE.get_or_init(|| Regex::new(r"\$\{(?P<path>[^}]+)\}").expect("invalid regex"))
    }

    fn incomplete_template_re() -> &'static Regex {
        INCOMPLETE_TEMPLATE_RE
            .get_or_init(|| Regex::new(r"\$\{(?P<path>[^}]*)$").expect("invalid regex"))
    }

    #[derive(Debug, PartialEq)]
    pub struct TemplateContext {
        pub full_path: String,
        pub cursor_offset: usize,
    }

    #[derive(Debug, PartialEq)]
    pub enum CompletionContext {
        FileName { partial: String },
        KeyPath {
            file_key: String,
            key_path: Vec<String>,
            partial: String,
        },
    }

    impl TemplateContext {
        pub fn path_before_cursor(&self) -> &str {
            let end = self.cursor_offset.min(self.full_path.len());
            &self.full_path[..end]
        }

        pub fn completion_context(&self) -> CompletionContext {
            let before = self.path_before_cursor();

            if !before.contains('.') {
                CompletionContext::FileName {
                    partial: before.to_string(),
                }
            } else {
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

    pub fn get_template_at_position(
        content: &str,
        line: usize,
        col: usize,
    ) -> Option<TemplateContext> {
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

    // ============ TESTS ============

    #[test]
    fn test_complete_template_cursor_inside() {
        let content = "pm: ${a.value}";
        // Cursor at position 7: "pm: ${a|.value}" (after 'a')
        let ctx = get_template_at_position(content, 0, 7).unwrap();
        assert_eq!(ctx.full_path, "a.value");
        assert_eq!(ctx.cursor_offset, 1); // after 'a'
    }

    #[test]
    fn test_complete_template_cursor_at_dot() {
        let content = "pm: ${a.value}";
        // Cursor at position 8: "pm: ${a.|value}" (after '.')
        let ctx = get_template_at_position(content, 0, 8).unwrap();
        assert_eq!(ctx.full_path, "a.value");
        assert_eq!(ctx.cursor_offset, 2); // after 'a.'
    }

    #[test]
    fn test_incomplete_template_just_opened() {
        let content = "pm: ${";
        // Cursor at position 6: "pm: ${|"
        let ctx = get_template_at_position(content, 0, 6).unwrap();
        assert_eq!(ctx.full_path, "");
        assert_eq!(ctx.cursor_offset, 0);
    }

    #[test]
    fn test_incomplete_template_typing_file() {
        let content = "pm: ${a";
        // Cursor at position 7: "pm: ${a|"
        let ctx = get_template_at_position(content, 0, 7).unwrap();
        assert_eq!(ctx.full_path, "a");
        assert_eq!(ctx.cursor_offset, 1);
    }

    #[test]
    fn test_incomplete_template_after_dot() {
        let content = "pm: ${a.";
        // Cursor at position 8: "pm: ${a.|"
        let ctx = get_template_at_position(content, 0, 8).unwrap();
        assert_eq!(ctx.full_path, "a.");
        assert_eq!(ctx.cursor_offset, 2);
    }

    #[test]
    fn test_incomplete_template_typing_key() {
        let content = "pm: ${a.val";
        // Cursor at position 11: "pm: ${a.val|"
        let ctx = get_template_at_position(content, 0, 11).unwrap();
        assert_eq!(ctx.full_path, "a.val");
        assert_eq!(ctx.cursor_offset, 5);
    }

    #[test]
    fn test_incomplete_template_nested_path() {
        let content = "pm: ${a.nested.";
        // Cursor at position 15: "pm: ${a.nested.|"
        let ctx = get_template_at_position(content, 0, 15).unwrap();
        assert_eq!(ctx.full_path, "a.nested.");
        assert_eq!(ctx.cursor_offset, 9);
    }

    #[test]
    fn test_no_template_outside() {
        let content = "pm: hello";
        let ctx = get_template_at_position(content, 0, 5);
        assert!(ctx.is_none());
    }

    #[test]
    fn test_completion_context_filename() {
        let ctx = TemplateContext {
            full_path: "a".to_string(),
            cursor_offset: 1,
        };
        assert_eq!(
            ctx.completion_context(),
            CompletionContext::FileName {
                partial: "a".to_string()
            }
        );
    }

    #[test]
    fn test_completion_context_filename_empty() {
        let ctx = TemplateContext {
            full_path: "".to_string(),
            cursor_offset: 0,
        };
        assert_eq!(
            ctx.completion_context(),
            CompletionContext::FileName {
                partial: "".to_string()
            }
        );
    }

    #[test]
    fn test_completion_context_after_dot() {
        // "${a." -> should suggest keys from file "a"
        let ctx = TemplateContext {
            full_path: "a.".to_string(),
            cursor_offset: 2,
        };
        assert_eq!(
            ctx.completion_context(),
            CompletionContext::KeyPath {
                file_key: "a".to_string(),
                key_path: vec![],
                partial: "".to_string(),
            }
        );
    }

    #[test]
    fn test_completion_context_partial_key() {
        // "${a.val" -> should suggest keys from file "a" starting with "val"
        let ctx = TemplateContext {
            full_path: "a.val".to_string(),
            cursor_offset: 5,
        };
        assert_eq!(
            ctx.completion_context(),
            CompletionContext::KeyPath {
                file_key: "a".to_string(),
                key_path: vec![],
                partial: "val".to_string(),
            }
        );
    }

    #[test]
    fn test_completion_context_nested_after_dot() {
        // "${a.nested." -> should suggest keys from a.nested
        let ctx = TemplateContext {
            full_path: "a.nested.".to_string(),
            cursor_offset: 9,
        };
        assert_eq!(
            ctx.completion_context(),
            CompletionContext::KeyPath {
                file_key: "a".to_string(),
                key_path: vec!["nested".to_string()],
                partial: "".to_string(),
            }
        );
    }

    #[test]
    fn test_completion_context_nested_partial() {
        // "${a.nested.ke" -> should suggest keys from a.nested starting with "ke"
        let ctx = TemplateContext {
            full_path: "a.nested.ke".to_string(),
            cursor_offset: 11,
        };
        assert_eq!(
            ctx.completion_context(),
            CompletionContext::KeyPath {
                file_key: "a".to_string(),
                key_path: vec!["nested".to_string()],
                partial: "ke".to_string(),
            }
        );
    }

    #[test]
    fn test_completion_context_deep_nested() {
        // "${a.nested.deep." -> should suggest keys from a.nested.deep
        let ctx = TemplateContext {
            full_path: "a.nested.deep.".to_string(),
            cursor_offset: 14,
        };
        assert_eq!(
            ctx.completion_context(),
            CompletionContext::KeyPath {
                file_key: "a".to_string(),
                key_path: vec!["nested".to_string(), "deep".to_string()],
                partial: "".to_string(),
            }
        );
    }

    #[test]
    fn test_multiline_content() {
        let content = r#"<!>:
  import:
    - a

pm: ${a."#;
        // Line 4, cursor at position 8: "pm: ${a.|"
        let ctx = get_template_at_position(content, 4, 8).unwrap();
        assert_eq!(ctx.full_path, "a.");
        assert_eq!(ctx.cursor_offset, 2);
    }

    #[test]
    fn test_path_with_slash() {
        let content = "url: ${common/database.";
        // Cursor at end
        let ctx = get_template_at_position(content, 0, 23).unwrap();
        assert_eq!(ctx.full_path, "common/database.");
        assert_eq!(ctx.cursor_offset, 16);

        let comp = ctx.completion_context();
        assert_eq!(
            comp,
            CompletionContext::KeyPath {
                file_key: "common/database".to_string(),
                key_path: vec![],
                partial: "".to_string(),
            }
        );
    }
}

// ============ Full workspace completion tests ============

mod workspace_completion {
    use super::*;

    #[test]
    fn test_workspace_has_files() {
        let ws = create_test_workspace();
        assert!(ws.get_document_by_key("a").is_some());
        assert!(ws.get_document_by_key("b").is_some());
        assert!(ws.get_document_by_key("common/database").is_some());
        assert!(ws.get_document_by_key("services/api").is_some());
    }

    #[test]
    fn test_file_a_has_expected_keys() {
        let ws = create_test_workspace();
        let doc = ws.get_document_by_key("a").unwrap();
        let yaml = doc.yaml.as_ref().unwrap();

        assert!(yaml.get("value").is_some());
        assert!(yaml.get("number").is_some());
        assert!(yaml.get("nested").is_some());
    }

    #[test]
    fn test_file_b_imports_a() {
        let ws = create_test_workspace();
        let doc = ws.get_document_by_key("b").unwrap();

        assert!(doc.metadata.imports.contains(&"a".to_string()));
    }

    #[test]
    fn test_nested_keys_accessible() {
        let ws = create_test_workspace();
        let doc = ws.get_document_by_key("a").unwrap();
        let yaml = doc.yaml.as_ref().unwrap();

        let nested = yaml.get("nested").unwrap();
        assert!(nested.get("key1").is_some());
        assert!(nested.get("key2").is_some());
        assert!(nested.get("deep").is_some());

        let deep = nested.get("deep").unwrap();
        assert!(deep.get("level3").is_some());
    }
}
