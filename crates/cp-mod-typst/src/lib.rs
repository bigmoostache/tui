pub mod cli_parser;
pub mod compiler;
pub mod packages;
pub mod templates;
mod tools_execute;

use cp_base::modules::Module;
use cp_base::panels::Panel;
use cp_base::state::{ContextType, State};
use cp_base::tools::{ParamType, ToolDefinition, ToolParam};
use cp_base::tools::{ToolResult, ToolUse};

pub struct TypstModule;

/// Templates live here — in the shared (version-controlled) folder.
pub const TEMPLATES_DIR: &str = ".context-pilot/shared/typst-templates";

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
        &["core", "callback"]
    }

    fn is_global(&self) -> bool {
        true
    }

    fn init_state(&self, state: &mut State) {
        cp_base::config::constants::ensure_shared_dir();
        ensure_typst_callback(state);
        templates::seed_templates();
    }

    fn reset_state(&self, _state: &mut State) {
        // No state to reset — stateless module
    }

    fn load_module_data(&self, _data: &serde_json::Value, state: &mut State) {
        cp_base::config::constants::ensure_shared_dir();
        ensure_typst_callback(state);
        templates::seed_templates();
    }

    fn tool_definitions(&self) -> Vec<ToolDefinition> {
        vec![ToolDefinition {
            id: "typst_execute".to_string(),
            name: "Execute Typst Command".to_string(),
            short_desc: "Run typst commands via embedded compiler".to_string(),
            description: "Executes typst CLI commands through the embedded compiler. No external typst binary needed. Supports: compile, init, query, fonts, update. Read-only commands (fonts, query) create a dynamic result panel. Mutating commands (compile, init, update) return output directly. Example: 'typst compile doc.typ -o out.pdf', 'typst init @preview/graceful-genetics:0.2.0', 'typst fonts'.".to_string(),
            params: vec![
                ToolParam::new("command", ParamType::String)
                    .desc("Full typst command string (e.g., 'typst compile doc.typ -o out.pdf', 'typst init @preview/graceful-genetics:0.2.0', 'typst fonts')")
                    .required(),
            ],
            enabled: true,
            category: "PDF".to_string(),
        }]
    }

    fn execute_tool(&self, tool: &ToolUse, state: &mut State) -> Option<ToolResult> {
        match tool.name.as_str() {
            "typst_execute" => Some(tools_execute::execute_typst(tool, state)),
            _ => None,
        }
    }

    fn create_panel(&self, _context_type: &ContextType) -> Option<Box<dyn Panel>> {
        None
    }

    fn tool_category_descriptions(&self) -> Vec<(&'static str, &'static str)> {
        vec![("PDF", "Create and manage Typst PDF documents")]
    }
}

/// Ensure the typst-compile callback exists in CallbackState.
/// Single callback: *.typ (excluding templates dir). Uses $CP_CHANGED_FILES
/// to compile each changed .typ → .pdf in the same directory.
fn ensure_typst_callback(state: &mut State) {
    use cp_mod_callback::types::{CallbackDefinition, CallbackState};

    let cs = CallbackState::get_mut(state);

    let binary_path = std::env::current_exe().unwrap_or_default().to_string_lossy().to_string();

    // Remove old callbacks from previous design (pdf/documents, pdf/templates patterns)
    cs.definitions.retain(|d| d.name != "typst-compile" && d.name != "typst-compile-template");
    cs.active_set.retain(|id| cs.definitions.iter().any(|d| &d.id == id));

    // Single callback: compile any *.typ → *.pdf (same dir)
    // The script skips files under the templates dir.
    let cb_id = format!("CB{}", cs.next_id);
    cs.next_id += 1;

    let script = format!(
        r#"bash -c 'echo "$CP_CHANGED_FILES" | while IFS= read -r FILE; do
  [ -z "$FILE" ] && continue
  case "$FILE" in .context-pilot/shared/typst-templates/*) continue;; esac
  {} typst-compile "$FILE"
done'"#,
        binary_path
    );

    cs.definitions.push(CallbackDefinition {
        id: cb_id.clone(),
        name: "typst-compile".to_string(),
        description: "Auto-compile .typ → .pdf (same directory) on edit".to_string(),
        pattern: "*.typ".to_string(),
        blocking: true,
        timeout_secs: Some(30),
        success_message: Some("✓ PDF compiled".to_string()),
        cwd: None,
        one_at_a_time: false,
        built_in: true,
        built_in_command: Some(script),
    });
    cs.active_set.insert(cb_id);
}
