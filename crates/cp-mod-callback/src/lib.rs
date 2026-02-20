// Arr! Callback module — auto-fires scripts when files walk the plank! ⚓
// Tested by the pirate crew on this fine day
mod panel;
pub mod tools;
pub mod trigger;
pub mod types;

use serde_json::json;

use cp_base::modules::Module;
use cp_base::panels::Panel;
use cp_base::state::{ContextType, ContextTypeMeta, State};
use cp_base::tools::{ParamType, ToolDefinition, ToolParam};
use cp_base::tools::{ToolResult, ToolUse};

use self::panel::CallbackPanel;
use self::types::CallbackState;

pub struct CallbackModule;

impl Module for CallbackModule {
    fn id(&self) -> &'static str {
        "callback"
    }
    fn name(&self) -> &'static str {
        "Callback"
    }
    fn description(&self) -> &'static str {
        "Auto-fire bash scripts when files are edited"
    }

    fn is_global(&self) -> bool {
        true
    }

    fn dependencies(&self) -> &[&'static str] {
        &["console"]
    }

    fn init_state(&self, state: &mut State) {
        state.set_ext(CallbackState::new());
    }

    fn reset_state(&self, state: &mut State) {
        state.set_ext(CallbackState::new());
    }

    fn save_module_data(&self, state: &State) -> serde_json::Value {
        let cs = CallbackState::get(state);
        json!({
            "definitions": cs.definitions,
            "next_id": cs.next_id,
        })
    }

    fn load_module_data(&self, data: &serde_json::Value, state: &mut State) {
        if let Some(defs) = data.get("definitions")
            && let Ok(v) = serde_json::from_value(defs.clone())
        {
            CallbackState::get_mut(state).definitions = v;
        }
        if let Some(v) = data.get("next_id").and_then(|v| v.as_u64()) {
            CallbackState::get_mut(state).next_id = v as usize;
        }
    }

    fn save_worker_data(&self, state: &State) -> serde_json::Value {
        let cs = CallbackState::get(state);
        let active: Vec<&String> = cs.active_set.iter().collect();
        json!({ "active_set": active })
    }

    fn load_worker_data(&self, data: &serde_json::Value, state: &mut State) {
        if let Some(arr) = data.get("active_set")
            && let Ok(v) = serde_json::from_value::<Vec<String>>(arr.clone())
        {
            CallbackState::get_mut(state).active_set = v.into_iter().collect();
        }
    }

    fn fixed_panel_types(&self) -> Vec<ContextType> {
        vec![ContextType::new(ContextType::CALLBACK)]
    }

    fn fixed_panel_defaults(&self) -> Vec<(ContextType, &'static str, bool)> {
        vec![(ContextType::new(ContextType::CALLBACK), "Callbacks", false)]
    }

    fn create_panel(&self, context_type: &ContextType) -> Option<Box<dyn Panel>> {
        match context_type.as_str() {
            ContextType::CALLBACK => Some(Box::new(CallbackPanel)),
            _ => None,
        }
    }

    fn context_type_metadata(&self) -> Vec<ContextTypeMeta> {
        vec![ContextTypeMeta {
            context_type: "callback",
            icon_id: "spine", // Reuse spine icon (⚡) for now
            is_fixed: true,
            needs_cache: false,
            fixed_order: Some(7),
            display_name: "callback",
            short_name: "callback",
            needs_async_wait: false,
        }]
    }

    fn tool_definitions(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                id: "Callback_upsert".to_string(),
                name: "Callback Upsert".to_string(),
                short_desc: "Create, update, or delete a callback".to_string(),
                description: "Creates, updates, or deletes a file edit callback. \
                    Callbacks are bash scripts that auto-fire when the AI edits files matching a glob pattern. \
                    Use action='create' to define a new callback with its script. \
                    Use action='update' to modify an existing callback. \
                    Use action='delete' to remove a callback and its script file."
                    .to_string(),
                params: vec![
                    ToolParam::new("action", ParamType::String)
                        .desc("Action: 'create', 'update', or 'delete'")
                        .enum_vals(&["create", "update", "delete"])
                        .required(),
                    ToolParam::new("id", ParamType::String)
                        .desc("Callback ID (required for update/delete, e.g. 'CB1')"),
                    ToolParam::new("name", ParamType::String)
                        .desc("Display name (e.g., 'rust-check'). Required for create."),
                    ToolParam::new("description", ParamType::String)
                        .desc("Short explanation of what this callback does"),
                    ToolParam::new("pattern", ParamType::String)
                        .desc("Gitignore-style glob (e.g., '*.rs', 'src/**/*.ts'). Required for create."),
                    ToolParam::new("script_content", ParamType::String)
                        .desc("Bash script body (shebang auto-prepended). Required for create."),
                    ToolParam::new("blocking", ParamType::Boolean)
                        .desc("Block Edit/Write result until script completes (default: false)"),
                    ToolParam::new("timeout", ParamType::Integer)
                        .desc("Max execution time in seconds (required if blocking)"),
                    ToolParam::new("success_message", ParamType::String)
                        .desc("Custom message on success (e.g., 'Build passed ✓')"),
                    ToolParam::new("cwd", ParamType::String)
                        .desc("Working directory (defaults to project root)"),
                    ToolParam::new("one_at_a_time", ParamType::Boolean)
                        .desc("Don't run simultaneously with itself (default: false)"),
                    ToolParam::new("old_string", ParamType::String)
                        .desc("For diff-based script update: exact text to find"),
                    ToolParam::new("new_string", ParamType::String)
                        .desc("For diff-based script update: replacement text"),
                ],
                enabled: true,
                category: "Callback".to_string(),
            },
            ToolDefinition {
                id: "Callback_open_editor".to_string(),
                name: "Callback Open Editor".to_string(),
                short_desc: "Open callback script in editor".to_string(),
                description: "Opens a callback's script content in the Callbacks panel for reading and editing. \
                    Required before using diff-based script editing (old_string/new_string in Callback_upsert update). \
                    Max one callback open at a time — opening a new one closes the previous."
                    .to_string(),
                params: vec![
                    ToolParam::new("id", ParamType::String)
                        .desc("Callback ID (e.g., 'CB1')")
                        .required(),
                ],
                enabled: true,
                category: "Callback".to_string(),
            },
            ToolDefinition {
                id: "Callback_close_editor".to_string(),
                name: "Callback Close Editor".to_string(),
                short_desc: "Close callback script editor".to_string(),
                description: "Closes the callback script editor in the Callbacks panel, restoring the normal table view."
                    .to_string(),
                params: vec![],
                enabled: true,
                category: "Callback".to_string(),
            },
            ToolDefinition {
                id: "Callback_toggle".to_string(),
                name: "Callback Toggle".to_string(),
                short_desc: "Activate/deactivate a callback for this worker".to_string(),
                description: "Activates or deactivates a callback for the current worker. \
                    Does NOT modify the callback definition — only this worker's activation state."
                    .to_string(),
                params: vec![
                    ToolParam::new("id", ParamType::String)
                        .desc("Callback ID (e.g., 'CB1')")
                        .required(),
                    ToolParam::new("active", ParamType::Boolean)
                        .desc("true to activate, false to deactivate")
                        .required(),
                ],
                enabled: true,
                category: "Callback".to_string(),
            },
        ]
    }

    fn execute_tool(&self, tool: &ToolUse, state: &mut State) -> Option<ToolResult> {
        match tool.name.as_str() {
            "Callback_upsert" => Some(self::tools::execute_upsert(tool, state)),
            "Callback_toggle" => Some(self::tools::execute_toggle(tool, state)),
            "Callback_open_editor" => Some(self::tools::execute_open_editor(tool, state)),
            "Callback_close_editor" => Some(self::tools::execute_close_editor(tool, state)),
            _ => None,
        }
    }

    fn context_detail(&self, ctx: &cp_base::state::ContextElement) -> Option<String> {
        if ctx.context_type.as_str() == ContextType::CALLBACK {
            let cs_count = "callbacks"; // Can't access state here, just label it
            Some(cs_count.to_string())
        } else {
            None
        }
    }

    fn tool_category_descriptions(&self) -> Vec<(&'static str, &'static str)> {
        vec![("Callback", "Auto-fire scripts on file edits")]
    }
}
