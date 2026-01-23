use std::collections::HashMap;
use std::sync::OnceLock;

use crate::functions::{registry, FunctionArg, FunctionError};
use crate::Value;

use regex::{Captures, Regex};

/// Regex for an exact match, e.g., "${a.b.c}" or "${a.b.c | func}"
static EXACT_MATCH_RE: OnceLock<Regex> = OnceLock::new();
/// Regex for finding all occurrences, e.g., in "http://${host}/${path}"
static INTERPOLATION_RE: OnceLock<Regex> = OnceLock::new();
/// Regex for parsing placeholder content: path and optional functions
static PLACEHOLDER_CONTENT_RE: OnceLock<Regex> = OnceLock::new();
/// Regex for parsing a single function call: name and optional argument
static FUNCTION_CALL_RE: OnceLock<Regex> = OnceLock::new();

fn exact_match_re() -> &'static Regex {
    EXACT_MATCH_RE.get_or_init(|| Regex::new(r"^\$\{(?P<content>[^}]+)\}$").expect("invalid regex"))
}

fn interpolation_re() -> &'static Regex {
    INTERPOLATION_RE
        .get_or_init(|| Regex::new(r"\$\{(?P<content>[^}]+)\}").expect("invalid regex"))
}

/// Returns the regex for matching template references: ${path.to.value}
///
/// This regex matches template placeholders like `${some.path}` and captures
/// the content inside the braces in a named group called "content".
///
/// Useful for LSP and other tools that need to find template references in text.
pub fn template_re() -> &'static Regex {
    interpolation_re()
}

/// A template reference found in a document, with position information.
///
/// This is useful for LSP features (diagnostics, go-to-definition) and
/// for providing detailed error messages with line/column information.
#[derive(Debug, Clone, PartialEq)]
pub struct TemplateRef {
    /// The content inside the template (e.g., "common/database.host" from "${common/database.host}")
    pub path: String,
    /// Line number (0-indexed)
    pub line: usize,
    /// Column where the template starts (0-indexed, points to '$')
    pub col_start: usize,
    /// Column where the template ends (0-indexed, points after '}')
    pub col_end: usize,
}

/// Find all template references in text content with their positions.
///
/// Scans the content line by line and returns all `${...}` template references
/// with their positions (line, column start, column end).
///
/// # Example
/// ```
/// use konf_provider::render_helper::find_template_refs;
///
/// let content = "host: ${db.host}\nport: ${db.port}";
/// let refs = find_template_refs(content);
///
/// assert_eq!(refs.len(), 2);
/// assert_eq!(refs[0].path, "db.host");
/// assert_eq!(refs[0].line, 0);
/// assert_eq!(refs[1].path, "db.port");
/// assert_eq!(refs[1].line, 1);
/// ```
pub fn find_template_refs(content: &str) -> Vec<TemplateRef> {
    let mut refs = vec![];

    for (line_idx, line) in content.lines().enumerate() {
        for cap in template_re().captures_iter(line) {
            if let Some(content_match) = cap.name("content") {
                let full_match = cap.get(0).unwrap();
                refs.push(TemplateRef {
                    path: content_match.as_str().to_string(),
                    line: line_idx,
                    col_start: full_match.start(),
                    col_end: full_match.end(),
                });
            }
        }
    }

    refs
}

fn placeholder_content_re() -> &'static Regex {
    PLACEHOLDER_CONTENT_RE.get_or_init(|| {
        // Matches: "path.to.value" or "path.to.value | func1 | func2:arg"
        Regex::new(r"^(?P<path>[\w./]+)(?P<funcs>\s*\|.+)?$").expect("invalid regex")
    })
}

fn function_call_re() -> &'static Regex {
    FUNCTION_CALL_RE.get_or_init(|| {
        // Matches: "funcname" or "funcname:\"arg\"" or "funcname:123" or "funcname:true"
        Regex::new(r#"(?P<name>\w+)(?::(?:"(?P<str_arg>[^"]*)"|(?P<num_arg>-?\d+(?:\.\d+)?)|(?P<bool_arg>true|false)))?"#)
            .expect("invalid regex")
    })
}

/// A parsed function call with name and optional argument.
#[derive(Debug)]
struct ParsedFunctionCall {
    name: String,
    arg: Option<FunctionArg>,
}

/// Parses a chain of function calls from a string like "| func1 | func2:\"arg\"".
fn parse_function_chain(chain: &str) -> Result<Vec<ParsedFunctionCall>, FunctionError> {
    let mut functions = Vec::new();

    // Split by pipe and process each function
    for part in chain.split('|') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }

        if let Some(caps) = function_call_re().captures(part) {
            let name = caps.name("name").unwrap().as_str().to_string();

            let arg = if let Some(str_arg) = caps.name("str_arg") {
                Some(FunctionArg::String(str_arg.as_str().to_string()))
            } else if let Some(num_arg) = caps.name("num_arg") {
                let num_str = num_arg.as_str();
                if num_str.contains('.') {
                    Some(FunctionArg::Float(num_str.parse().unwrap_or(0.0)))
                } else {
                    Some(FunctionArg::Int(num_str.parse().unwrap_or(0)))
                }
            } else {
                caps.name("bool_arg")
                    .map(|bool_arg| FunctionArg::Boolean(bool_arg.as_str() == "true"))
            };

            functions.push(ParsedFunctionCall { name, arg });
        }
    }

    Ok(functions)
}

/// Applies a chain of functions to a value.
fn apply_function_chain(
    mut value: Value,
    funcs: &[ParsedFunctionCall],
) -> Result<Value, FunctionError> {
    let reg = registry();

    for func in funcs {
        let args: Vec<FunctionArg> = func.arg.iter().cloned().collect();
        value = reg.execute(&func.name, value, &args)?;
    }

    Ok(value)
}

/// Resolves a placeholder expression (path + optional functions) against dependencies.
/// Returns None if the path cannot be resolved.
fn resolve_placeholder_expression(
    expr: &str,
    deps: &HashMap<String, Value>,
) -> Option<Result<Value, FunctionError>> {
    let content_caps = placeholder_content_re().captures(expr)?;

    let path = content_caps.name("path")?.as_str();

    // Look up the value
    let value = lookup_in_deps(path, deps)?;

    // Check if there are functions to apply
    let funcs_str = content_caps.name("funcs").map(|m| m.as_str());

    match funcs_str {
        Some(chain) => {
            // Parse and apply function chain
            match parse_function_chain(chain) {
                Ok(funcs) if funcs.is_empty() => Some(Ok(value.clone())),
                Ok(funcs) => Some(apply_function_chain(value.clone(), &funcs)),
                Err(e) => Some(Err(e)),
            }
        }
        None => Some(Ok(value.clone())),
    }
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

/// Helper to stringify a `Value` for interpolation.
/// Complex types like Mappings and Sequences return None as they can't be
/// meaningfully embedded in a string.
fn value_to_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Int(n) => Some(n.to_string()),
        Value::Float(n) => Some(n.to_string()),
        Value::Boolean(b) => Some(b.to_string()),
        Value::Null => Some("null".to_string()),
        // Sequences and Mappings can't be meaningfully embedded in a string
        Value::Sequence(_) | Value::Mapping(_) => None,
    }
}

/// Traverses a `serde_yaml::Value` and replaces any `"${path}"` strings
/// with the corresponding values found in the `deps` map.
/// Supports function chains like `${path.to.value | trim | upper}`.
pub fn resolve_refs_from_deps(value: &mut Value, deps: &HashMap<String, Value>) {
    match value {
        Value::String(s) => {
            // Case 1: The entire string is a single placeholder, like "${a.b.c}" or "${a.b.c | func}".
            // In this case, we replace the string with the referenced value, preserving its type.
            if let Some(caps) = exact_match_re().captures(s) {
                if let Some(content) = caps.name("content")
                    && let Some(result) = resolve_placeholder_expression(content.as_str(), deps)
                {
                    match result {
                        Ok(replacement) => {
                            *value = replacement;
                        }
                        Err(e) => {
                            // Log error but leave placeholder unchanged
                            tracing::warn!("Function error in placeholder: {}", e);
                        }
                    }
                }
                // Stop processing to avoid falling through to interpolation logic.
                return;
            }

            // Case 2: The string contains one or more placeholders for interpolation,
            // like "http://${server.host}:${server.port}/path".
            // The result will always be a new string.
            let new_s = interpolation_re().replace_all(s, |caps: &Captures| {
                // Get the content from the "content" capture group.
                caps.name("content")
                    .and_then(|content| resolve_placeholder_expression(content.as_str(), deps))
                    .and_then(|result| result.ok())
                    .and_then(|v| value_to_string(&v))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::functions::FunctionArg;
    use crate::Mapping;

    fn make_mapping(entries: Vec<(&str, Value)>) -> Mapping {
        entries
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect()
    }

    #[test]
    fn test_parse_function_chain_single() {
        let funcs = parse_function_chain("| trim").unwrap();
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].name, "trim");
        assert!(funcs[0].arg.is_none());
    }

    #[test]
    fn test_parse_function_chain_multiple() {
        let funcs = parse_function_chain("| trim | upper | lower").unwrap();
        assert_eq!(funcs.len(), 3);
        assert_eq!(funcs[0].name, "trim");
        assert_eq!(funcs[1].name, "upper");
        assert_eq!(funcs[2].name, "lower");
    }

    #[test]
    fn test_parse_function_chain_with_string_arg() {
        let funcs = parse_function_chain(r#"| default:"fallback""#).unwrap();
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].name, "default");
        assert!(matches!(&funcs[0].arg, Some(FunctionArg::String(s)) if s == "fallback"));
    }

    #[test]
    fn test_parse_function_chain_with_int_arg() {
        let funcs = parse_function_chain("| someFunc:42").unwrap();
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].name, "someFunc");
        assert!(matches!(funcs[0].arg, Some(FunctionArg::Int(42))));
    }

    #[test]
    fn test_parse_function_chain_with_float_arg() {
        let funcs = parse_function_chain("| someFunc:3.14").unwrap();
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].name, "someFunc");
        assert!(matches!(funcs[0].arg, Some(FunctionArg::Float(f)) if (f - 3.14).abs() < 0.001));
    }

    #[test]
    fn test_parse_function_chain_with_bool_arg() {
        let funcs = parse_function_chain("| someFunc:true").unwrap();
        assert_eq!(funcs.len(), 1);
        assert!(matches!(funcs[0].arg, Some(FunctionArg::Boolean(true))));

        let funcs = parse_function_chain("| someFunc:false").unwrap();
        assert!(matches!(funcs[0].arg, Some(FunctionArg::Boolean(false))));
    }

    #[test]
    fn test_resolve_refs_simple() {
        let mut deps = HashMap::new();
        deps.insert(
            "base".to_string(),
            Value::Mapping(make_mapping(vec![
                ("name", Value::String("hello".to_string())),
            ])),
        );

        let mut value = Value::String("${base.name}".to_string());
        resolve_refs_from_deps(&mut value, &deps);
        assert_eq!(value, Value::String("hello".to_string()));
    }

    #[test]
    fn test_resolve_refs_with_function() {
        let mut deps = HashMap::new();
        deps.insert(
            "base".to_string(),
            Value::Mapping(make_mapping(vec![
                ("name", Value::String("  hello  ".to_string())),
            ])),
        );

        let mut value = Value::String("${base.name | trim}".to_string());
        resolve_refs_from_deps(&mut value, &deps);
        assert_eq!(value, Value::String("hello".to_string()));
    }

    #[test]
    fn test_resolve_refs_with_chained_functions() {
        let mut deps = HashMap::new();
        deps.insert(
            "base".to_string(),
            Value::Mapping(make_mapping(vec![
                ("name", Value::String("  hello  ".to_string())),
            ])),
        );

        let mut value = Value::String("${base.name | trim | upper}".to_string());
        resolve_refs_from_deps(&mut value, &deps);
        assert_eq!(value, Value::String("HELLO".to_string()));
    }

    #[test]
    fn test_resolve_refs_interpolation_with_function() {
        let mut deps = HashMap::new();
        deps.insert(
            "base".to_string(),
            Value::Mapping(make_mapping(vec![
                ("name", Value::String("hello world".to_string())),
            ])),
        );

        let mut value =
            Value::String("url: https://example.com?name=${base.name | url_escape}".to_string());
        resolve_refs_from_deps(&mut value, &deps);
        assert_eq!(
            value,
            Value::String("url: https://example.com?name=hello%20world".to_string())
        );
    }

    #[test]
    fn test_resolve_refs_default_function() {
        let mut deps = HashMap::new();
        deps.insert(
            "base".to_string(),
            Value::Mapping(make_mapping(vec![
                ("existing", Value::String("value".to_string())),
                ("null_val", Value::Null),
            ])),
        );

        // Default not applied when value exists
        let mut value = Value::String(r#"${base.existing | default:"fallback"}"#.to_string());
        resolve_refs_from_deps(&mut value, &deps);
        assert_eq!(value, Value::String("value".to_string()));

        // Default applied when value is null
        let mut value = Value::String(r#"${base.null_val | default:"fallback"}"#.to_string());
        resolve_refs_from_deps(&mut value, &deps);
        assert_eq!(value, Value::String("fallback".to_string()));
    }

    #[test]
    fn test_resolve_refs_unknown_path_unchanged() {
        let deps = HashMap::new();

        let mut value = Value::String("${unknown.path}".to_string());
        resolve_refs_from_deps(&mut value, &deps);
        assert_eq!(value, Value::String("${unknown.path}".to_string()));
    }

    #[test]
    fn test_resolve_refs_preserves_type() {
        let mut deps = HashMap::new();
        deps.insert(
            "base".to_string(),
            Value::Mapping(make_mapping(vec![
                ("number", Value::Int(42)),
                ("flag", Value::Boolean(true)),
            ])),
        );

        // Int preserved
        let mut value = Value::String("${base.number}".to_string());
        resolve_refs_from_deps(&mut value, &deps);
        assert_eq!(value, Value::Int(42));

        // Boolean preserved
        let mut value = Value::String("${base.flag}".to_string());
        resolve_refs_from_deps(&mut value, &deps);
        assert_eq!(value, Value::Boolean(true));
    }
}
