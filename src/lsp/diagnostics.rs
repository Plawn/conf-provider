//! Diagnostics provider for konf-lsp
//!
//! Provides error and warning diagnostics for:
//! - Invalid import references
//! - Invalid template references
//! - Circular imports
//! - Type warnings (complex types in string interpolation)

use std::collections::HashSet;

use tower_lsp::lsp_types::*;

use super::parser::parse_template_path;
use super::workspace::Workspace;

/// Get diagnostics for a document
pub fn get_diagnostics(ws: &Workspace, uri: &Url) -> Vec<Diagnostic> {
    let Some(doc) = ws.get_document(uri) else {
        return vec![];
    };

    let mut diagnostics = vec![];

    // Check for YAML parse errors
    if doc.yaml.is_none() {
        diagnostics.push(Diagnostic {
            range: Range {
                start: Position::new(0, 0),
                end: Position::new(0, 1),
            },
            severity: Some(DiagnosticSeverity::ERROR),
            code: Some(NumberOrString::String("yaml-parse-error".to_string())),
            source: Some("konf-lsp".to_string()),
            message: "Failed to parse YAML".to_string(),
            ..Default::default()
        });
        return diagnostics;
    }

    // Check imports
    diagnostics.extend(check_imports(ws, doc));

    // Check template references
    diagnostics.extend(check_template_refs(ws, doc));

    // Check for circular imports
    diagnostics.extend(check_circular_imports(ws, doc));

    diagnostics
}

/// Check that all imports reference valid files
fn check_imports(ws: &Workspace, doc: &super::parser::KonfDocument) -> Vec<Diagnostic> {
    let mut diagnostics = vec![];

    // Check each import in the metadata
    for import_info in doc.metadata.imports.values() {
        let resolved_path = import_info.resolved_path.as_ref().unwrap_or(&import_info.path);

        // Check if the imported file exists
        if !ws.has_key(resolved_path) {
            // Find the line containing this import for error positioning
            for (line_idx, line) in doc.content.lines().enumerate() {
                if line.contains(&import_info.path) && super::parser::is_in_import_section(&doc.content, line_idx) {
                    diagnostics.push(Diagnostic {
                        range: Range {
                            start: Position::new(line_idx as u32, 0),
                            end: Position::new(line_idx as u32, line.len() as u32),
                        },
                        severity: Some(DiagnosticSeverity::ERROR),
                        code: Some(NumberOrString::String("unknown-import".to_string())),
                        source: Some("konf-lsp".to_string()),
                        message: format!("Unknown import: '{}' (resolved to '{}')", import_info.path, resolved_path),
                        ..Default::default()
                    });
                    break;
                }
            }
        }

        // Check for self-import
        if resolved_path == &doc.key {
            for (line_idx, line) in doc.content.lines().enumerate() {
                if line.contains(&import_info.path) && super::parser::is_in_import_section(&doc.content, line_idx) {
                    diagnostics.push(Diagnostic {
                        range: Range {
                            start: Position::new(line_idx as u32, 0),
                            end: Position::new(line_idx as u32, line.len() as u32),
                        },
                        severity: Some(DiagnosticSeverity::ERROR),
                        code: Some(NumberOrString::String("self-import".to_string())),
                        source: Some("konf-lsp".to_string()),
                        message: "Cannot import self".to_string(),
                        ..Default::default()
                    });
                    break;
                }
            }
        }
    }

    diagnostics
}

/// Check that all template references are valid
fn check_template_refs(ws: &Workspace, doc: &super::parser::KonfDocument) -> Vec<Diagnostic> {
    let mut diagnostics = vec![];

    for tref in &doc.template_refs {
        let Some((alias, key_path)) = parse_template_path(&tref.path) else {
            continue;
        };

        // Check if the alias is imported
        let Some(import_info) = doc.metadata.imports.get(&alias) else {
            diagnostics.push(Diagnostic {
                range: Range {
                    start: Position::new(tref.line as u32, tref.col_start as u32),
                    end: Position::new(tref.line as u32, tref.col_end as u32),
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(NumberOrString::String("unimported-reference".to_string())),
                source: Some("konf-lsp".to_string()),
                message: format!(
                    "Reference to '{alias}' but it is not imported. Add it to the import section."
                ),
                ..Default::default()
            });
            continue;
        };

        // Get the resolved path
        let resolved_path = import_info.resolved_path.as_ref().unwrap_or(&import_info.path);

        // Check if the referenced file exists
        let Some(ref_doc) = ws.get_document_by_key(resolved_path) else {
            // Already reported by import check
            continue;
        };

        // Check if the key path exists
        let path_refs: Vec<&str> = key_path.iter().map(|s| s.as_str()).collect();
        if ref_doc.get_value_at_path(&path_refs).is_none() {
            diagnostics.push(Diagnostic {
                range: Range {
                    start: Position::new(tref.line as u32, tref.col_start as u32),
                    end: Position::new(tref.line as u32, tref.col_end as u32),
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(NumberOrString::String("unknown-key".to_string())),
                source: Some("konf-lsp".to_string()),
                message: format!(
                    "Key '{}' not found in '{}' ({})",
                    key_path.join("."),
                    alias,
                    resolved_path
                ),
                ..Default::default()
            });
            continue;
        }

        // Check for interpolation of complex types
        let line_content = doc.content.lines().nth(tref.line).unwrap_or("");
        let is_exact_match = line_content
            .trim()
            .split(':')
            .nth(1)
            .map(|v| v.trim() == format!("${{{}}}", tref.path))
            .unwrap_or(false);

        if !is_exact_match {
            // This is string interpolation, check if the type is complex
            if let Some(value) = ref_doc.get_value_at_path(&path_refs) {
                if matches!(
                    value,
                    serde_yaml::Value::Mapping(_) | serde_yaml::Value::Sequence(_)
                ) {
                    diagnostics.push(Diagnostic {
                        range: Range {
                            start: Position::new(tref.line as u32, tref.col_start as u32),
                            end: Position::new(tref.line as u32, tref.col_end as u32),
                        },
                        severity: Some(DiagnosticSeverity::WARNING),
                        code: Some(NumberOrString::String("complex-interpolation".to_string())),
                        source: Some("konf-lsp".to_string()),
                        message: format!(
                            "Cannot interpolate complex type ({}) in string. Use exact match instead.",
                            if matches!(value, serde_yaml::Value::Mapping(_)) {
                                "Mapping"
                            } else {
                                "Sequence"
                            }
                        ),
                        ..Default::default()
                    });
                }
            }
        }
    }

    diagnostics
}

/// Check for circular imports
fn check_circular_imports(ws: &Workspace, doc: &super::parser::KonfDocument) -> Vec<Diagnostic> {
    let mut diagnostics = vec![];
    let mut visited = HashSet::new();
    let mut path = vec![doc.key.clone()];

    if let Some(cycle) = detect_cycle(ws, &doc.key, &mut visited, &mut path) {
        // Find the import line for the first import in the cycle
        let cycle_str = cycle.join(" -> ");

        // Find any import line to report the error
        for (line_idx, line) in doc.content.lines().enumerate() {
            if super::parser::is_in_import_section(&doc.content, line_idx) {
                // Check if this line contains any path from the cycle
                for import_info in doc.metadata.imports.values() {
                    let resolved = import_info.resolved_path.as_ref().unwrap_or(&import_info.path);
                    if cycle.contains(resolved) && line.contains(&import_info.path) {
                        diagnostics.push(Diagnostic {
                            range: Range {
                                start: Position::new(line_idx as u32, 0),
                                end: Position::new(line_idx as u32, line.len() as u32),
                            },
                            severity: Some(DiagnosticSeverity::ERROR),
                            code: Some(NumberOrString::String("circular-import".to_string())),
                            source: Some("konf-lsp".to_string()),
                            message: format!("Circular import detected: {cycle_str}"),
                            ..Default::default()
                        });
                        return diagnostics;
                    }
                }
            }
        }
    }

    diagnostics
}

/// Detect import cycles using DFS
fn detect_cycle(
    ws: &Workspace,
    key: &str,
    visited: &mut HashSet<String>,
    path: &mut Vec<String>,
) -> Option<Vec<String>> {
    if visited.contains(key) {
        // Found a cycle, return the path from the first occurrence
        if let Some(pos) = path.iter().position(|k| k == key) {
            return Some(path[pos..].to_vec());
        }
        return None;
    }

    visited.insert(key.to_string());

    if let Some(doc) = ws.get_document_by_key(key) {
        for import_info in doc.metadata.imports.values() {
            let resolved = import_info.resolved_path.as_ref().unwrap_or(&import_info.path);
            path.push(resolved.clone());
            if let Some(cycle) = detect_cycle(ws, resolved, visited, path) {
                return Some(cycle);
            }
            path.pop();
        }
    }

    None
}
