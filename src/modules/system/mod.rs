pub mod types;
pub mod seed;
mod panel;
pub(crate) mod tools;

use serde_json::json;

use crate::core::panels::Panel;
use crate::state::{ContextType, State};
use crate::tool_defs::{ToolDefinition, ToolParam, ParamType, ToolCategory};
use crate::tools::{ToolUse, ToolResult};

use self::panel::SystemPanel;
use super::Module;

pub struct SystemModule;

impl Module for SystemModule {
    fn id(&self) -> &'static str { "system" }
    fn name(&self) -> &'static str { "System" }
    fn description(&self) -> &'static str { "System prompt management" }
    fn is_core(&self) -> bool { true }
    fn is_global(&self) -> bool { true }

    fn save_module_data(&self, state: &State) -> serde_json::Value {
        json!({
            "systems": state.systems,
            "next_system_id": state.next_system_id,
            "active_system_id": state.active_system_id,
        })
    }

    fn load_module_data(&self, data: &serde_json::Value, state: &mut State) {
        if let Some(arr) = data.get("systems") {
            if let Ok(v) = serde_json::from_value(arr.clone()) {
                state.systems = v;
            }
        }
        if let Some(v) = data.get("next_system_id").and_then(|v| v.as_u64()) {
            state.next_system_id = v as usize;
        }
        if let Some(v) = data.get("active_system_id") {
            state.active_system_id = v.as_str().map(String::from);
        }
    }

    fn fixed_panel_types(&self) -> Vec<ContextType> {
        vec![ContextType::System]
    }

    fn fixed_panel_defaults(&self) -> Vec<(ContextType, &'static str, bool)> {
        vec![(ContextType::System, "Seed", false)]
    }

    fn create_panel(&self, context_type: ContextType) -> Option<Box<dyn Panel>> {
        match context_type {
            ContextType::System => Some(Box::new(SystemPanel)),
            _ => None,
        }
    }

    fn tool_definitions(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                id: "system_create".to_string(),
                name: "Create System".to_string(),
                short_desc: "Create system prompt".to_string(),
                description: "Creates a new system prompt with a name, description, and content. System prompts define the agent's identity and behavior.".to_string(),
                params: vec![
                    ToolParam::new("name", ParamType::String)
                        .desc("System prompt name")
                        .required(),
                    ToolParam::new("description", ParamType::String)
                        .desc("Short description of this system prompt"),
                    ToolParam::new("content", ParamType::String)
                        .desc("Full system prompt content")
                        .required(),
                ],
                enabled: true,
                category: ToolCategory::Context,
            },
            ToolDefinition {
                id: "system_edit".to_string(),
                name: "Edit System".to_string(),
                short_desc: "Edit system prompt".to_string(),
                description: "Edits an existing system prompt. Can update name, description, or content.".to_string(),
                params: vec![
                    ToolParam::new("id", ParamType::String)
                        .desc("System prompt ID (e.g., S0, S1)")
                        .required(),
                    ToolParam::new("name", ParamType::String)
                        .desc("New name"),
                    ToolParam::new("description", ParamType::String)
                        .desc("New description"),
                    ToolParam::new("content", ParamType::String)
                        .desc("New content"),
                ],
                enabled: true,
                category: ToolCategory::Context,
            },
            ToolDefinition {
                id: "system_delete".to_string(),
                name: "Delete System".to_string(),
                short_desc: "Delete system prompt".to_string(),
                description: "Deletes a system prompt. If the deleted prompt was active, reverts to default.".to_string(),
                params: vec![
                    ToolParam::new("id", ParamType::String)
                        .desc("System prompt ID to delete (e.g., S0)")
                        .required(),
                ],
                enabled: true,
                category: ToolCategory::Context,
            },
            ToolDefinition {
                id: "system_load".to_string(),
                name: "Load System".to_string(),
                short_desc: "Activate system prompt".to_string(),
                description: "Loads/activates a system prompt. Pass empty id to revert to default system prompt.".to_string(),
                params: vec![
                    ToolParam::new("id", ParamType::String)
                        .desc("System prompt ID to activate (e.g., S0). Empty to use default."),
                ],
                enabled: true,
                category: ToolCategory::Context,
            },
        ]
    }

    fn execute_tool(&self, tool: &ToolUse, state: &mut State) -> Option<ToolResult> {
        match tool.name.as_str() {
            "system_create" => Some(self::tools::create_system(tool, state)),
            "system_edit" => Some(self::tools::edit_system(tool, state)),
            "system_delete" => Some(self::tools::delete_system(tool, state)),
            "system_load" => Some(self::tools::load_system(tool, state)),
            _ => None,
        }
    }
}
