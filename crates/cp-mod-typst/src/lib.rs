pub mod compiler;
mod templates;
mod tools;
pub mod types;

use serde_json::json;

use cp_base::modules::Module;
use cp_base::panels::Panel;
use cp_base::state::{ContextType, State};
use cp_base::tools::{ParamType, ToolDefinition, ToolParam};
use cp_base::tools::{ToolResult, ToolUse};

use crate::types::TypstState;

pub struct TypstModule;

impl Module for TypstModule {
    fn id(&self) -> &'static str {
        "typst"
    }

    fn name(&self) -> &'static str {
        "Typst PDF"
    }

    fn description(&self) -> &'static str {
        "PDF generation via embedded Typst compiler"
    }

    fn dependencies(&self) -> &[&'static str] {
        &["core", "callback", "tree"]
    }

    fn is_global(&self) -> bool {
        true // Documents config is shared across workers
    }

    fn init_state(&self, state: &mut State) {
        state.set_ext(TypstState::new());
    }

    fn reset_state(&self, state: &mut State) {
        state.set_ext(TypstState::new());
    }

    fn save_module_data(&self, state: &State) -> serde_json::Value {
        let ts = TypstState::get(state);
        json!({
            "documents": ts.documents,
            "templates_seeded": ts.templates_seeded,
        })
    }

    fn load_module_data(&self, data: &serde_json::Value, state: &mut State) {
        let ts = TypstState::get_mut(state);
        if let Some(docs) = data.get("documents") {
            if let Ok(d) = serde_json::from_value(docs.clone()) {
                ts.documents = d;
            }
        }
        if let Some(v) = data.get("templates_seeded").and_then(|v| v.as_bool()) {
            ts.templates_seeded = v;
        }
        // Always ensure the typst-compile callback is registered at startup.
        // Don't trust a boolean flag — verify the callback actually exists in CallbackState.
        tools::ensure_typst_callback(state);
    }

    fn tool_definitions(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                id: "pdf_create".to_string(),
                name: "Create PDF Document".to_string(),
                short_desc: "Create a new Typst document".to_string(),
                description: "Creates a new .typ document in .context-pilot/pdf/documents/, registers it with a target PDF path, and opens the file for editing. Use the standard Edit tool to write content afterward.".to_string(),
                params: vec![
                    ToolParam::new("name", ParamType::String)
                        .desc("Document name (used as filename and identifier)")
                        .required(),
                    ToolParam::new("target", ParamType::String)
                        .desc("Target path for the compiled PDF (e.g., './reports/q1.pdf')")
                        .required(),
                    ToolParam::new("template", ParamType::String)
                        .desc("Template name to base the document on (e.g., 'report', 'invoice', 'letter')"),
                ],
                enabled: true,
                category: "PDF".to_string(),
            },
            ToolDefinition {
                id: "pdf_edit".to_string(),
                name: "Edit PDF Metadata".to_string(),
                short_desc: "Update or delete PDF document config".to_string(),
                description: "Updates document metadata (target path) or deletes a document (source, output, target, and config). Does NOT edit .typ content — use the standard Edit tool for that.".to_string(),
                params: vec![
                    ToolParam::new("name", ParamType::String)
                        .desc("Document name")
                        .required(),
                    ToolParam::new("target", ParamType::String)
                        .desc("New target path for the compiled PDF"),
                    ToolParam::new("delete", ParamType::Boolean)
                        .desc("Set true to delete this document (removes source, output, target PDF, and config entry)"),
                ],
                enabled: true,
                category: "PDF".to_string(),
            },
        ]
    }

    fn execute_tool(&self, tool: &ToolUse, state: &mut State) -> Option<ToolResult> {
        match tool.name.as_str() {
            "pdf_create" => Some(tools::execute_create(tool, state)),
            "pdf_edit" => Some(tools::execute_edit(tool, state)),
            _ => None,
        }
    }

    fn create_panel(&self, _context_type: &ContextType) -> Option<Box<dyn Panel>> {
        None // No custom panel — lean on tree integration
    }

    fn tool_category_descriptions(&self) -> Vec<(&'static str, &'static str)> {
        vec![("PDF", "Create and manage Typst PDF documents")]
    }

    fn overview_context_section(&self, state: &State) -> Option<String> {
        let ts = TypstState::get(state);
        if ts.documents.is_empty() {
            return None;
        }
        let doc_list: Vec<String> = ts.documents.values().map(|d| format!("  {} → {}", d.name, d.target)).collect();
        Some(format!("PDF Documents ({}):\n{}\n", ts.documents.len(), doc_list.join("\n")))
    }
}
