mod panel;
mod tools;

use cp_base::panels::Panel;
use cp_base::state::{ContextType, State};
use cp_base::tool_defs::{ParamType, ToolDefinition, ToolParam};
use cp_base::tools::{ToolResult, ToolUse};

use self::panel::GlobPanel;
use cp_base::modules::Module;

pub struct GlobModule;

impl Module for GlobModule {
    fn id(&self) -> &'static str {
        "glob"
    }
    fn name(&self) -> &'static str {
        "Glob"
    }
    fn description(&self) -> &'static str {
        "File pattern matching search"
    }

    fn dynamic_panel_types(&self) -> Vec<ContextType> {
        vec![ContextType::new(ContextType::GLOB)]
    }

    fn create_panel(&self, context_type: &ContextType) -> Option<Box<dyn Panel>> {
        match context_type.as_str() {
            ContextType::GLOB => Some(Box::new(GlobPanel)),
            _ => None,
        }
    }

    fn tool_definitions(&self) -> Vec<ToolDefinition> {
        vec![ToolDefinition {
            id: "file_glob".to_string(),
            name: "Glob Search".to_string(),
            short_desc: "Find files by pattern".to_string(),
            description: "Searches for files matching a glob pattern. Results are added to context.".to_string(),
            params: vec![
                ToolParam::new("pattern", ParamType::String)
                    .desc("Glob pattern (e.g., '**/*.rs', 'src/*.ts')")
                    .required(),
                ToolParam::new("path", ParamType::String).desc("Base path to search from").default_val("."),
            ],
            enabled: true,
            category: "File".to_string(),
        }]
    }

    fn execute_tool(&self, tool: &ToolUse, state: &mut State) -> Option<ToolResult> {
        match tool.name.as_str() {
            "file_glob" => Some(self::tools::execute(tool, state)),
            _ => None,
        }
    }

    fn context_type_metadata(&self) -> Vec<cp_base::state::ContextTypeMeta> {
        vec![cp_base::state::ContextTypeMeta {
            context_type: "glob",
            icon_id: "glob",
            is_fixed: false,
            needs_cache: true,
            fixed_order: None,
            display_name: "glob",
            short_name: "glob",
            needs_async_wait: false,
        }]
    }

    fn context_detail(&self, ctx: &cp_base::state::ContextElement) -> Option<String> {
        if ctx.context_type.as_str() == cp_base::state::ContextType::GLOB {
            Some(ctx.get_meta_str("glob_pattern").unwrap_or("").to_string())
        } else {
            None
        }
    }

    fn watch_paths(&self, state: &State) -> Vec<cp_base::panels::WatchSpec> {
        state
            .context
            .iter()
            .filter(|c| c.context_type.as_str() == ContextType::GLOB)
            .map(|c| cp_base::panels::WatchSpec::Dir(c.get_meta_str("glob_path").unwrap_or(".").to_string()))
            .collect()
    }

    fn should_invalidate_on_fs_change(
        &self,
        ctx: &cp_base::state::ContextElement,
        changed_path: &str,
        is_dir_event: bool,
    ) -> bool {
        if !is_dir_event || ctx.context_type.as_str() != ContextType::GLOB {
            return false;
        }
        let base = ctx.get_meta_str("glob_path").unwrap_or(".");
        changed_path.starts_with(base) || base.starts_with(changed_path)
    }
}
