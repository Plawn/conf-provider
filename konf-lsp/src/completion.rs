//! Completion provider for konf-lsp
//!
//! Provides autocompletion for:
//! - Template references: ${file.key.path}
//! - Import paths in <!>: section

use tower_lsp::lsp_types::*;

use crate::parser::{
    get_template_at_position, is_in_import_section, parse_template_path, CompletionContext,
};
use crate::workspace::Workspace;

/// Get completion items for the current position
pub fn get_completions(ws: &Workspace, uri: &Url, position: Position) -> Vec<CompletionItem> {
    let Some(doc) = ws.get_document(uri) else {
        tracing::warn!("No document found for URI: {}", uri);
        return vec![];
    };

    let line = position.line as usize;
    let col = position.character as usize;

    let line_content = doc.content.lines().nth(line).unwrap_or("");
    tracing::info!(
        "Completion requested at line {}, col {}: {:?}",
        line,
        col,
        line_content
    );

    // Check if we're in a template reference
    if let Some(ctx) = get_template_at_position(&doc.content, line, col) {
        tracing::info!("Template context found: {:?}", ctx.completion_context());
        return get_template_completions(ws, doc, ctx.completion_context(), position);
    }

    // Check if we're in the import section
    if is_in_import_section(&doc.content, line) {
        tracing::info!("In import section");
        return get_import_completions(ws, doc);
    }

    // Check if we just typed ${ (trigger character)
    let before_cursor = &line_content[..col.min(line_content.len())];

    if before_cursor.ends_with("${") {
        tracing::info!("Just typed ${{, suggesting imported files");
        // Just started a template, suggest imported files
        return get_template_completions(
            ws,
            doc,
            CompletionContext::FileName {
                partial: String::new(),
            },
            position,
        );
    }

    tracing::info!("No completion context found");
    vec![]
}

/// Get completions for template references
fn get_template_completions(
    ws: &Workspace,
    doc: &crate::parser::KonfDocument,
    ctx: CompletionContext,
    position: Position,
) -> Vec<CompletionItem> {
    match ctx {
        CompletionContext::FileName { partial } => {
            tracing::info!(
                "FileName completion: partial={:?}, imports={:?}",
                partial,
                doc.metadata.imports.keys().collect::<Vec<_>>()
            );

            // Calculate the range to replace (the partial text typed so far)
            let start_col = position.character - partial.len() as u32;
            let range = Range {
                start: Position::new(position.line, start_col),
                end: position,
            };

            // Suggest import aliases that match the partial
            doc.metadata
                .imports
                .values()
                .filter(|imp| imp.alias.starts_with(&partial))
                .map(|imp| CompletionItem {
                    label: imp.alias.clone(),
                    kind: Some(CompletionItemKind::FILE),
                    detail: Some(format!("-> {}", imp.path)),
                    filter_text: Some(imp.alias.clone()),
                    text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                        range,
                        new_text: imp.alias.clone(),
                    })),
                    ..Default::default()
                })
                .collect()
        }
        CompletionContext::KeyPath {
            file_key,
            key_path,
            partial,
        } => {
            tracing::info!(
                "KeyPath completion: file_key={:?}, key_path={:?}, partial={:?}",
                file_key,
                key_path,
                partial
            );

            // Find the import info for this alias
            let Some(import_info) = doc.metadata.imports.get(&file_key) else {
                tracing::warn!("Import alias not found: {}", file_key);
                return vec![];
            };

            // Get the resolved path for the import
            let resolved_path = import_info.resolved_path.as_ref().unwrap_or(&import_info.path);

            // Get the referenced document using the resolved path
            let Some(ref_doc) = ws.get_document_by_key(resolved_path) else {
                tracing::warn!("Referenced document not found: {} (resolved from {})", resolved_path, import_info.path);
                tracing::info!("Available keys: {:?}", ws.get_all_keys());
                return vec![];
            };

            // Get keys at the current path
            let path_refs: Vec<&str> = key_path.iter().map(|s| s.as_str()).collect();
            let keys = ref_doc.get_keys_at_path(&path_refs);
            tracing::info!("Found keys: {:?}", keys.iter().map(|k| &k.name).collect::<Vec<_>>());

            // Calculate the range to replace (the partial text typed so far)
            let start_col = position.character - partial.len() as u32;
            let range = Range {
                start: Position::new(position.line, start_col),
                end: position,
            };

            // Return all keys, let VSCode filter based on what's typed
            let items: Vec<CompletionItem> = keys
                .into_iter()
                .map(|k| {
                    let is_mapping = k.value_type == "Mapping";
                    CompletionItem {
                        label: k.name.clone(),
                        kind: Some(if is_mapping {
                            CompletionItemKind::MODULE
                        } else {
                            CompletionItemKind::FIELD
                        }),
                        detail: Some(k.value_type.clone()),
                        documentation: Some(Documentation::String(k.preview.clone())),
                        filter_text: Some(k.name.clone()),
                        sort_text: Some(k.name.clone()),
                        text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                            range,
                            new_text: k.name.clone(),
                        })),
                        ..Default::default()
                    }
                })
                .collect();

            tracing::info!("Returning {} completion items: {:?}", items.len(), items.iter().map(|i| &i.label).collect::<Vec<_>>());
            items
        }
    }
}

/// Get completions for import paths
fn get_import_completions(
    ws: &Workspace,
    doc: &crate::parser::KonfDocument,
) -> Vec<CompletionItem> {
    // Get all available config keys
    let all_keys = ws.get_all_keys();

    // Filter out already imported files (by resolved path) and the current file
    let already_imported: std::collections::HashSet<&str> = doc
        .metadata
        .imports
        .values()
        .filter_map(|info| info.resolved_path.as_deref())
        .collect();

    all_keys
        .into_iter()
        .filter(|key| {
            let key_str = key.as_str();
            !already_imported.contains(key_str) && key_str != doc.key
        })
        .map(|key| CompletionItem {
            label: key.clone(),
            kind: Some(CompletionItemKind::FILE),
            detail: Some("Config file".to_string()),
            ..Default::default()
        })
        .collect()
}

/// Go to definition for template references and imports
pub fn goto_definition(ws: &Workspace, uri: &Url, position: Position) -> Option<Location> {
    let doc = ws.get_document(uri)?;
    let line = position.line as usize;
    let col = position.character as usize;

    // Check if cursor is on a template reference
    if let Some(ctx) = get_template_at_position(&doc.content, line, col) {
        let (file_key, _key_path) = parse_template_path(&ctx.full_path)?;

        // Find the referenced file
        let target_uri = ws.get_uri_for_key(&file_key)?;
        let target_url = Url::parse(target_uri).ok()?;

        return Some(Location {
            uri: target_url,
            range: Range {
                start: Position::new(0, 0),
                end: Position::new(0, 0),
            },
        });
    }

    // Check if cursor is on an import line
    let line_content = doc.content.lines().nth(line)?;
    let trimmed = line_content.trim();

    if trimmed.starts_with("- ") && is_in_import_section(&doc.content, line) {
        let import_key = trimmed.trim_start_matches("- ").trim();
        let target_uri = ws.get_uri_for_key(import_key)?;
        let target_url = Url::parse(target_uri).ok()?;

        return Some(Location {
            uri: target_url,
            range: Range {
                start: Position::new(0, 0),
                end: Position::new(0, 0),
            },
        });
    }

    None
}

/// Provide hover information
pub fn hover(ws: &Workspace, uri: &Url, position: Position) -> Option<Hover> {
    let doc = ws.get_document(uri)?;
    let line = position.line as usize;
    let col = position.character as usize;

    // Check if cursor is on a template reference
    if let Some(ctx) = get_template_at_position(&doc.content, line, col) {
        let (file_key, key_path) = parse_template_path(&ctx.full_path)?;

        // Find the referenced document
        let ref_doc = ws.get_document_by_key(&file_key)?;

        // Get the value at the path
        let path_refs: Vec<&str> = key_path.iter().map(|s| s.as_str()).collect();
        let value = ref_doc.get_value_at_path(&path_refs)?;

        // Format the hover content
        let preview = format_yaml_preview(value);
        let content = format!(
            "**Source:** `{}`\n\n```yaml\n{}\n```",
            file_key, preview
        );

        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: content,
            }),
            range: None,
        });
    }

    None
}

/// Format a YAML value for preview
fn format_yaml_preview(value: &serde_yaml::Value) -> String {
    serde_yaml::to_string(value).unwrap_or_else(|_| "...".to_string())
}
