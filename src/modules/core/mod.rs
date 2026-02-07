mod system_panel;
mod conversation_panel;
mod overview_panel;
mod tools;

use serde_json::json;

use crate::core::panels::Panel;
use crate::state::{ContextType, State};
use crate::tool_defs::{ToolDefinition, ToolParam, ParamType, ToolCategory};
use crate::tools::{ToolUse, ToolResult};

use self::system_panel::SystemPanel;
use self::conversation_panel::ConversationPanel;
use self::overview_panel::OverviewPanel;
use super::Module;

pub struct CoreModule;

impl Module for CoreModule {
    fn id(&self) -> &'static str { "core" }
    fn name(&self) -> &'static str { "Core" }
    fn description(&self) -> &'static str { "Essential context and system tools" }
    fn is_core(&self) -> bool { true }
    fn is_global(&self) -> bool { true }

    fn save_module_data(&self, state: &State) -> serde_json::Value {
        json!({
            "systems": state.systems,
            "next_system_id": state.next_system_id,
            "active_modules": state.active_modules.iter().collect::<Vec<_>>(),
            "dev_mode": state.dev_mode,
            "llm_provider": state.llm_provider,
            "anthropic_model": state.anthropic_model,
            "grok_model": state.grok_model,
            "groq_model": state.groq_model,
            "cleaning_threshold": state.cleaning_threshold,
            "cleaning_target_proportion": state.cleaning_target_proportion,
            "context_budget": state.context_budget,
            "global_next_uid": state.global_next_uid,
            "disabled_tools": state.tools.iter().filter(|t| !t.enabled).map(|t| &t.id).collect::<Vec<_>>(),
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
        if let Some(arr) = data.get("active_modules").and_then(|v| v.as_array()) {
            state.active_modules = arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
        }
        if let Some(v) = data.get("dev_mode").and_then(|v| v.as_bool()) {
            state.dev_mode = v;
        }
        if let Some(v) = data.get("llm_provider") {
            if let Ok(p) = serde_json::from_value(v.clone()) {
                state.llm_provider = p;
            }
        }
        if let Some(v) = data.get("anthropic_model") {
            if let Ok(m) = serde_json::from_value(v.clone()) {
                state.anthropic_model = m;
            }
        }
        if let Some(v) = data.get("grok_model") {
            if let Ok(m) = serde_json::from_value(v.clone()) {
                state.grok_model = m;
            }
        }
        if let Some(v) = data.get("groq_model") {
            if let Ok(m) = serde_json::from_value(v.clone()) {
                state.groq_model = m;
            }
        }
        if let Some(v) = data.get("cleaning_threshold").and_then(|v| v.as_f64()) {
            state.cleaning_threshold = v as f32;
        }
        if let Some(v) = data.get("cleaning_target_proportion").and_then(|v| v.as_f64()) {
            state.cleaning_target_proportion = v as f32;
        }
        if let Some(v) = data.get("context_budget") {
            state.context_budget = v.as_u64().map(|n| n as usize);
        }
        if let Some(v) = data.get("global_next_uid").and_then(|v| v.as_u64()) {
            state.global_next_uid = v as usize;
        }
        if let Some(arr) = data.get("disabled_tools").and_then(|v| v.as_array()) {
            let disabled: Vec<String> = arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
            // Build tools from active_modules (must be loaded already) and apply disabled state
            state.tools = crate::modules::active_tool_definitions(&state.active_modules);
            for tool in &mut state.tools {
                if tool.id != "tool_manage" && tool.id != "module_toggle" && disabled.contains(&tool.id) {
                    tool.enabled = false;
                }
            }
        }
        if let Some(v) = data.get("active_system_id") {
            state.active_system_id = v.as_str().map(String::from);
        }
    }

    fn fixed_panel_types(&self) -> Vec<ContextType> {
        vec![
            ContextType::System,
            ContextType::Conversation,
            ContextType::Overview,
        ]
    }

    fn create_panel(&self, context_type: ContextType) -> Option<Box<dyn Panel>> {
        match context_type {
            ContextType::System => Some(Box::new(SystemPanel)),
            ContextType::Conversation => Some(Box::new(ConversationPanel)),
            ContextType::Overview => Some(Box::new(OverviewPanel)),
            _ => None,
        }
    }

    fn tool_definitions(&self) -> Vec<ToolDefinition> {
        let mut defs = vec![
            // Context tools
            ToolDefinition {
                id: "context_close".to_string(),
                name: "Close Contexts".to_string(),
                short_desc: "Remove items from context".to_string(),
                description: "Closes context elements by their IDs (e.g., P6, P7). Cannot close core elements (P1-P6).".to_string(),
                params: vec![
                    ToolParam::new("ids", ParamType::Array(Box::new(ParamType::String)))
                        .desc("List of context IDs to close")
                        .required(),
                ],
                enabled: true,
                category: ToolCategory::Context,
            },
            ToolDefinition {
                id: "context_message_status".to_string(),
                name: "Message Status".to_string(),
                short_desc: "Manage message visibility".to_string(),
                description: "Changes message status to control what's sent to the LLM. Batched.".to_string(),
                params: vec![
                    ToolParam::new("changes", ParamType::Array(Box::new(ParamType::Object(vec![
                        ToolParam::new("message_id", ParamType::String)
                            .desc("Message ID (e.g., U1, A3)")
                            .required(),
                        ToolParam::new("status", ParamType::String)
                            .desc("full | summarized | deleted")
                            .required(),
                        ToolParam::new("tl_dr", ParamType::String)
                            .desc("Required when status is 'summarized'"),
                    ]))))
                        .desc("Array of status changes")
                        .required(),
                ],
                enabled: true,
                category: ToolCategory::Context,
            },

            // System prompt tools
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

            // System tools
            ToolDefinition {
                id: "system_reload".to_string(),
                name: "Reload TUI".to_string(),
                short_desc: "Restart the TUI".to_string(),
                description: "Reloads the TUI application to apply changes. Use after modifying TUI source code and rebuilding. State is preserved. IMPORTANT: You must ALWAYS call this tool after building - never just say 'reloading' without actually invoking this tool.".to_string(),
                params: vec![],
                enabled: true,
                category: ToolCategory::Context,
            },

            // Meta tools
            ToolDefinition {
                id: "tool_manage".to_string(),
                name: "Manage Tools".to_string(),
                short_desc: "Enable/disable tools".to_string(),
                description: "Enables or disables tools. This tool cannot be disabled. Use to customize available capabilities.".to_string(),
                params: vec![
                    ToolParam::new("changes", ParamType::Array(Box::new(ParamType::Object(vec![
                        ToolParam::new("tool", ParamType::String)
                            .desc("Tool ID to change (e.g., 'edit_file', 'glob')")
                            .required(),
                        ToolParam::new("action", ParamType::String)
                            .desc("Action to perform")
                            .enum_vals(&["enable", "disable"])
                            .required(),
                    ]))))
                        .desc("Array of tool changes")
                        .required(),
                ],
                enabled: true,
                category: ToolCategory::Context,
            },
        ];

        // Add module_toggle tool
        defs.push(super::module_toggle_tool_definition());

        defs
    }

    fn execute_tool(&self, tool: &ToolUse, state: &mut State) -> Option<ToolResult> {
        match tool.name.as_str() {
            // Context tools
            "context_close" => Some(self::tools::close_context::execute(tool, state)),
            "context_message_status" => Some(self::tools::message_status::execute(tool, state)),

            // System prompt tools
            "system_create" => Some(self::tools::system::create_system(tool, state)),
            "system_edit" => Some(self::tools::system::edit_system(tool, state)),
            "system_delete" => Some(self::tools::system::delete_system(tool, state)),
            "system_load" => Some(self::tools::system::load_system(tool, state)),

            // System tools
            "system_reload" => Some(crate::tools::execute_reload_tui(tool, state)),

            // Meta tools
            "tool_manage" => Some(self::tools::manage_tools::execute(tool, state)),

            // module_toggle is handled in dispatch_tool() directly
            _ => None,
        }
    }
}
