use cp_base::state::{ContextElement, ContextType, State};
use cp_base::tools::{ToolResult, ToolUse};
use std::fs;
use std::path::Path;

use crate::templates::seed_templates;
use crate::types::{TypstDocument, TypstState};
use cp_mod_tree::{TreeFileDescription, TreeState};

const DOCUMENTS_DIR: &str = ".context-pilot/pdf/documents";
const TEMPLATES_DIR: &str = ".context-pilot/pdf/templates";

/// Execute pdf_create tool.
pub fn execute_create(tool: &ToolUse, state: &mut State) -> ToolResult {
    let name = match tool.input.get("name").and_then(|v| v.as_str()) {
        Some(n) => n.to_string(),
        None => {
            return ToolResult {
                tool_use_id: tool.id.clone(),
                content: "Error: missing required 'name' parameter".to_string(),
                is_error: true,
                tool_name: tool.name.clone(),
            };
        }
    };

    let target = match tool.input.get("target").and_then(|v| v.as_str()) {
        Some(t) => t.to_string(),
        None => {
            return ToolResult {
                tool_use_id: tool.id.clone(),
                content: "Error: missing required 'target' parameter (destination path for compiled PDF)".to_string(),
                is_error: true,
                tool_name: tool.name.clone(),
            };
        }
    };

    let template = tool.input.get("template").and_then(|v| v.as_str()).map(|s| s.to_string());

    // Check if document already exists
    {
        let typst_state = TypstState::get(state);
        if typst_state.documents.contains_key(&name) {
            return ToolResult {
                tool_use_id: tool.id.clone(),
                content: format!("Error: document '{}' already exists. Use pdf_edit to modify.", name),
                is_error: true,
                tool_name: tool.name.clone(),
            };
        }
    }

    // Ensure directory structure
    let _ = fs::create_dir_all(DOCUMENTS_DIR);
    let _ = fs::create_dir_all(TEMPLATES_DIR);

    // Seed built-in templates if not done yet
    let typst_state = TypstState::get_mut(state);
    if !typst_state.templates_seeded {
        seed_templates();
        typst_state.templates_seeded = true;
    }

    // Register the typst-compile callback if not present in CallbackState.
    // Always verify — don't trust flags, since callbacks can be deleted externally.
    ensure_typst_callback(state);

    // Build the .typ content
    let source_path = format!("{}/{}.typ", DOCUMENTS_DIR, name);
    let content = if let Some(ref tpl_name) = template {
        let tpl_path = format!("../templates/{}.typ", tpl_name);
        let tpl_file = format!("{}/{}.typ", TEMPLATES_DIR, tpl_name);
        if !Path::new(&tpl_file).exists() {
            return ToolResult {
                tool_use_id: tool.id.clone(),
                content: format!("Error: template '{}' not found. Available templates: {}", tpl_name, list_templates()),
                is_error: true,
                tool_name: tool.name.clone(),
            };
        }
        format!(
            "#import \"{}\": *\n\n// Document: {}\n// Target: {}\n\n= {}\n\nYour content here.\n",
            tpl_path, name, target, name
        )
    } else {
        format!("// Document: {}\n// Target: {}\n\n= {}\n\nYour content here.\n", name, target, name)
    };

    // Write the .typ file
    if let Err(e) = fs::write(&source_path, &content) {
        return ToolResult {
            tool_use_id: tool.id.clone(),
            content: format!("Error writing {}: {}", source_path, e),
            is_error: true,
            tool_name: tool.name.clone(),
        };
    }

    // Register document in TypstState (persisted by Module trait)
    let doc = TypstDocument {
        name: name.clone(),
        source: source_path.clone(),
        target: target.clone(),
        template: template.clone(),
    };
    {
        let typst_state = TypstState::get_mut(state);
        typst_state.documents.insert(name.clone(), doc);
    }

    // Annotate target path in tree panel so user can see which .typ file produces each PDF
    annotate_tree_target(state, &target, &source_path);

    // Auto-open the .typ file in context (same pattern as files module's Open tool)
    let file_name = format!("{}.typ", name);
    let context_id = state.next_available_context_id();
    let uid = format!("UID_{}_P", state.global_next_uid);
    state.global_next_uid += 1;

    let mut elem = ContextElement {
        id: context_id.clone(),
        uid: Some(uid),
        context_type: ContextType::new(ContextType::FILE),
        name: file_name,
        token_count: 0,
        metadata: std::collections::HashMap::new(),
        cached_content: None,
        history_messages: None,
        cache_deprecated: true,
        cache_in_flight: false,
        last_refresh_ms: cp_base::panels::now_ms(),
        content_hash: None,
        source_hash: None,
        current_page: 0,
        total_pages: 1,
        full_token_count: 0,
        panel_cache_hit: false,
        panel_total_cost: 0.0,
    };
    elem.set_meta("file_path", &source_path);
    state.context.push(elem);

    // Compile the initial PDF so the target exists immediately
    let compile_msg = match crate::compiler::compile_and_write(&source_path, &target) {
        Ok(msg) => format!("\n{}", msg),
        Err(e) => format!("\nWarning: initial compile failed: {}", e),
    };

    let mut result_msg = format!("Created document '{}'\n  Source: {}\n  Target: {}\n", name, source_path, target);
    if let Some(tpl) = &template {
        result_msg.push_str(&format!("  Template: {}\n", tpl));
    }
    result_msg.push_str(&compile_msg);
    result_msg.push_str(&format!("\nFile opened: {}\nUse Edit tool to write the document content.", source_path));

    ToolResult { tool_use_id: tool.id.clone(), content: result_msg, is_error: false, tool_name: tool.name.clone() }
}

/// Execute pdf_edit tool (metadata upsert / delete).
pub fn execute_edit(tool: &ToolUse, state: &mut State) -> ToolResult {
    let name = match tool.input.get("name").and_then(|v| v.as_str()) {
        Some(n) => n.to_string(),
        None => {
            return ToolResult {
                tool_use_id: tool.id.clone(),
                content: "Error: missing required 'name' parameter".to_string(),
                is_error: true,
                tool_name: tool.name.clone(),
            };
        }
    };

    // Delete mode
    if tool.input.get("delete").and_then(|v| v.as_bool()).unwrap_or(false) {
        let doc = {
            let typst_state = TypstState::get_mut(state);
            match typst_state.documents.remove(&name) {
                Some(d) => d,
                None => {
                    return ToolResult {
                        tool_use_id: tool.id.clone(),
                        content: format!("Error: document '{}' not found", name),
                        is_error: true,
                        tool_name: tool.name.clone(),
                    };
                }
            }
        };

        // Clean up files
        let _ = fs::remove_file(&doc.source);
        let _ = fs::remove_file(&doc.target);

        // Remove tree annotation for the target
        remove_tree_annotation(state, &doc.target);

        return ToolResult {
            tool_use_id: tool.id.clone(),
            content: format!(
                "Deleted document '{}'\n  Removed: {}\n  Removed: {}",
                name, doc.source, doc.target
            ),
            is_error: false,
            tool_name: tool.name.clone(),
        };
    }

    // Update mode — collect values first, then mutate state
    let (old_target, source_path, new_target_opt) = {
        let typst_state = TypstState::get_mut(state);
        let doc = match typst_state.documents.get_mut(&name) {
            Some(d) => d,
            None => {
                return ToolResult {
                    tool_use_id: tool.id.clone(),
                    content: format!(
                        "Error: document '{}' not found. Use pdf_create to create a new document.",
                        name
                    ),
                    is_error: true,
                    tool_name: tool.name.clone(),
                };
            }
        };

        let old_target = doc.target.clone();
        let source_path = doc.source.clone();
        let new_target_opt = tool.input.get("target").and_then(|v| v.as_str()).map(|s| s.to_string());

        // Apply target change to the document record
        if let Some(ref new_target) = new_target_opt {
            doc.target = new_target.clone();
        }

        (old_target, source_path, new_target_opt)
    };

    // Now perform filesystem and tree operations outside the TypstState borrow
    let mut changes = Vec::new();

    if let Some(ref new_target) = new_target_opt {
        // Move the compiled PDF to the new target path
        if Path::new(&old_target).exists() {
            if let Some(parent) = Path::new(new_target.as_str()).parent() {
                let _ = fs::create_dir_all(parent);
            }
            match fs::rename(&old_target, new_target.as_str()) {
                Ok(_) => changes.push(format!("  moved: {} → {}", old_target, new_target)),
                Err(e) => changes.push(format!("  target updated (move failed: {})", e)),
            }
        } else {
            changes.push(format!("  target: {} → {} (no PDF to move yet)", old_target, new_target));
        }

        // Update tree annotations
        remove_tree_annotation(state, &old_target);
        annotate_tree_target(state, new_target, &source_path);
    }

    if changes.is_empty() {
        return ToolResult {
            tool_use_id: tool.id.clone(),
            content: format!("No changes specified for document '{}'", name),
            is_error: true,
            tool_name: tool.name.clone(),
        };
    }

    ToolResult {
        tool_use_id: tool.id.clone(),
        content: format!("Updated document '{}':\n{}", name, changes.join("\n")),
        is_error: false,
        tool_name: tool.name.clone(),
    }
}

/// List available template names.
fn list_templates() -> String {
    let templates_dir = Path::new(TEMPLATES_DIR);
    if !templates_dir.exists() {
        return "(none)".to_string();
    }
    let mut names: Vec<String> = Vec::new();
    if let Ok(entries) = fs::read_dir(templates_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "typ") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    names.push(stem.to_string());
                }
            }
        }
    }
    if names.is_empty() {
        "(none)".to_string()
    } else {
        names.sort();
        names.join(", ")
    }
}

/// Annotate a PDF target path in the tree panel with its .typ source.
/// Shows something like: `q1-report.pdf  # → .context-pilot/pdf/documents/q1-report.typ`
fn annotate_tree_target(state: &mut State, target_path: &str, source_path: &str) {
    let ts = TreeState::get_mut(state);

    // Remove existing description for this path (if re-creating)
    ts.tree_descriptions.retain(|d| d.path != target_path);

    ts.tree_descriptions.push(TreeFileDescription {
        path: target_path.to_string(),
        description: format!("→ edit: {}", source_path),
        file_hash: String::new(), // no hash tracking needed for PDF targets
    });
}

/// Remove tree annotation for a PDF target path (on document delete).
fn remove_tree_annotation(state: &mut State, target_path: &str) {
    let ts = TreeState::get_mut(state);
    ts.tree_descriptions.retain(|d| d.path != target_path);
}

/// Ensure the typst-compile callback exists in CallbackState.
/// Called at module init (load_module_data) and on pdf_create.
/// If the callback was deleted externally, this re-creates it.
pub fn ensure_typst_callback(state: &mut State) {
    use cp_mod_callback::types::{CallbackDefinition, CallbackState};

    let cs = CallbackState::get_mut(state);

    // Resolve the current binary path so the callback invokes *this* compiled binary
    let binary_path = std::env::current_exe().unwrap_or_default().to_string_lossy().to_string();

    // 1. Document compile callback
    if !cs.definitions.iter().any(|d| d.name == "typst-compile") {
        let cb_id = format!("CB{}", cs.next_id);
        cs.next_id += 1;

        let built_in_cmd = format!(
            "bash -c 'echo \"$CP_CHANGED_FILES\" | while IFS= read -r FILE; do [ -n \"$FILE\" ] && {} typst-compile \"$FILE\"; done'",
            binary_path
        );

        cs.definitions.push(CallbackDefinition {
            id: cb_id.clone(),
            name: "typst-compile".to_string(),
            description: "Auto-compile .typ files to PDF on edit".to_string(),
            pattern: ".context-pilot/pdf/documents/*.typ".to_string(),
            blocking: true,
            timeout_secs: Some(30),
            success_message: Some("✓ PDF compiled".to_string()),
            cwd: None,
            one_at_a_time: false,
            built_in: true,
            built_in_command: Some(built_in_cmd),
        });
        cs.active_set.insert(cb_id);
    }

    // 2. Template recompile callback
    if !cs.definitions.iter().any(|d| d.name == "typst-compile-template") {
        let tpl_cb_id = format!("CB{}", cs.next_id);
        cs.next_id += 1;

        let tpl_cmd = format!(
            "bash -c 'echo \"$CP_CHANGED_FILES\" | while IFS= read -r FILE; do [ -n \"$FILE\" ] && {} typst-compile-template \"$FILE\"; done'",
            binary_path
        );

        cs.definitions.push(CallbackDefinition {
            id: tpl_cb_id.clone(),
            name: "typst-compile-template".to_string(),
            description: "Recompile all documents using an edited template".to_string(),
            pattern: ".context-pilot/pdf/templates/*.typ".to_string(),
            blocking: true,
            timeout_secs: Some(30),
            success_message: Some("✓ Template docs recompiled".to_string()),
            cwd: None,
            one_at_a_time: false,
            built_in: true,
            built_in_command: Some(tpl_cmd),
        });
        cs.active_set.insert(tpl_cb_id);
    }
}
