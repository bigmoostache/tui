// Silent callback test
pub mod cli_parser;
pub mod compiler;
pub mod packages;
pub mod templates;
mod tools_execute;
pub mod watchlist;

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
            description: concat!(
                "Executes typst CLI commands through the embedded compiler. No external typst binary needed. ",
                "Supports: compile, init, query, fonts, update, watch, unwatch, watchlist.\n\n",
                "SUBCOMMANDS:\n",
                "- 'typst compile <file.typ> [-o <output.pdf>]' — Compile a .typ file to PDF. Output defaults to same name with .pdf extension.\n",
                "- 'typst init @preview/<template>:<version> [directory]' — Download a template from Typst Universe and scaffold a project. Example: 'typst init @preview/brilliant-cv:2.0.3'.\n",
                "- 'typst watch <file.typ>' — Add a document to the auto-compile watchlist. Compiles immediately and records ALL dependencies (imports, images, bib files, toml, packages). ",
                "From then on, editing ANY dependency auto-recompiles the PDF via a callback. Use this for documents the user is actively working on.\n",
                "- 'typst unwatch <file.typ>' — Remove a document from the watchlist.\n",
                "- 'typst watchlist' — List all watched documents and their dependency counts.\n",
                "- 'typst fonts [--variants]' — List available system fonts (creates a panel).\n",
                "- 'typst query <file.typ> <selector>' — Query document metadata (creates a panel).\n",
                "- 'typst update [@preview/pkg:version]' — Re-download cached packages.\n\n",
                "FILE ORGANIZATION — CRITICAL PRINCIPLES:\n",
                "Context Pilot is installed into the user's project. You MUST keep their workspace clean. The guiding principle: ",
                "separate CONTENT (what the user cares about) from STYLING (reusable infrastructure). Content lives next to the document. Styling lives in .context-pilot/shared/.\n\n",
                "What goes in .context-pilot/shared/typst-templates/:\n",
                "- Template files (#let report = ..., #let invoice = ...) — reusable across documents\n",
                "- Shared styling (fonts, colors, layouts, page setup, headers/footers)\n",
                "- Shared assets used by templates (logos, icons, common images)\n",
                "- Shared data (company info TOML, common variables)\n",
                "These are version-controlled and shared across all workers. They are infrastructure, not user content.\n\n",
                "What goes next to the document (in the user's project folder):\n",
                "- The .typ document itself — containing ONLY content (text, data, tables)\n",
                "- Content-specific assets (photos for THIS report, data files for THIS document)\n",
                "- The compiled .pdf output\n",
                "- Configuration (metadata.toml, bibliography .bib) specific to THIS document\n",
                "Keep the user's folders minimal — only what the user authored or needs to see.\n\n",
                "When using 'typst init' to scaffold from @preview/ templates:\n",
                "- The template's reusable styling/library files should go into .context-pilot/shared/typst-templates/\n",
                "- Only the user's content files (the actual document, data, images) should go in the project directory\n",
                "- This prevents template scaffolding from dumping dozens of infrastructure files into the user's workspace\n\n",
                "WORKFLOW — BEST PRACTICES:\n",
                "1. After creating or scaffolding a .typ document, ALWAYS run 'typst watch' on the main document so edits auto-recompile.\n",
                "2. When the user asks to create a PDF, first check if a suitable template exists in .context-pilot/shared/typst-templates/. If not, create one there, then create the document that imports it.\n",
                "3. Documents should #import templates: #import \"../../.context-pilot/shared/typst-templates/report.typ\": report\n",
                "4. 'typst watch' tracks the FULL dependency tree automatically — imports, images, bibliography files, toml config, even package files. No manual dependency management needed.\n",
                "5. Use 'typst compile' for one-off compilations. Use 'typst watch' for documents under active development.\n",
                "6. @preview/ packages are downloaded from Typst Universe and cached globally at ~/.cache/typst/packages/.",
            ).to_string(),
            params: vec![
                ToolParam::new("command", ParamType::String)
                    .desc("Full typst command string (e.g., 'typst compile doc.typ -o out.pdf', 'typst init @preview/graceful-genetics:0.2.0', 'typst fonts')")
                    .required(),
            ],
            enabled: true,
            reverie_allowed: false,
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

/// Ensure the typst watchlist callback exists in CallbackState.
/// Single callback that watches ALL files (*) and checks against the watchlist's dependency trees.
fn ensure_typst_callback(state: &mut State) {
    use cp_mod_callback::types::{CallbackDefinition, CallbackState};

    let cs = CallbackState::get_mut(state);

    let binary_path = std::env::current_exe().unwrap_or_default().to_string_lossy().to_string();

    // Remove old callbacks from previous designs
    cs.definitions.retain(|d| {
        d.name != "typst-compile"
            && d.name != "typst-compile-template"
            && d.name != "typst-template-recompile"
            && d.name != "typst-watchlist"
    });
    cs.active_set.retain(|id| cs.definitions.iter().any(|d| &d.id == id));

    // Single callback: watches ALL files, checks watchlist to find affected docs
    let cb_id = format!("CB{}", cs.next_id);
    cs.next_id += 1;

    // The CLI subcommand reads the watchlist, checks if any changed files are dependencies,
    // and recompiles affected documents (updating deps at the same time).
    let script = format!(r#"bash -c '{} typst-recompile-watched $CP_CHANGED_FILES'"#, binary_path);

    cs.definitions.push(CallbackDefinition {
        id: cb_id.clone(),
        name: "typst-watchlist".to_string(),
        description: "Recompile watched .typ documents when their dependencies change".to_string(),
        pattern: "*".to_string(),
        blocking: true,
        timeout_secs: Some(60),
        success_message: None,
        cwd: None,
        one_at_a_time: false,
        built_in: true,
        built_in_command: Some(script),
    });
    cs.active_set.insert(cb_id);
}
