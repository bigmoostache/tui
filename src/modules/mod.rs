pub mod core;
pub mod files;
pub mod git;
pub mod glob;
pub mod grep;
pub mod memory;
pub mod scratchpad;
pub mod tmux;
pub mod todo;
pub mod tree;

use std::collections::HashSet;

use crate::core::panels::Panel;
use crate::state::{ContextType, State};
use crate::tool_defs::{ToolDefinition, ToolParam, ParamType, ToolCategory};
use crate::tools::{ToolUse, ToolResult};

/// A module that provides tools, panels, and configuration to the TUI.
///
/// Modules are stateless — all runtime state lives in `State`.
/// Activation/deactivation is a config toggle that controls whether
/// the module's tools and panels are registered.
pub trait Module: Send + Sync {
    /// Unique identifier (e.g., "core", "git", "tmux")
    fn id(&self) -> &'static str;
    /// Display name
    fn name(&self) -> &'static str;
    /// Short description
    fn description(&self) -> &'static str;
    /// IDs of modules this one depends on
    fn dependencies(&self) -> &[&'static str] { &[] }
    /// Core modules cannot be deactivated
    fn is_core(&self) -> bool { false }

    /// Whether this module's data is global (config.json) or per-worker (worker.json)
    fn is_global(&self) -> bool { false }

    /// Serialize this module's data from State into a JSON value for persistence.
    /// Returns Value::Null if this module has no data to persist.
    fn save_module_data(&self, _state: &State) -> serde_json::Value { serde_json::Value::Null }

    /// Deserialize this module's data from a JSON value and apply it to State.
    fn load_module_data(&self, _data: &serde_json::Value, _state: &mut State) {}

    /// Tool definitions provided by this module
    fn tool_definitions(&self) -> Vec<ToolDefinition>;
    /// Execute a tool. Returns None if this module doesn't own the tool.
    fn execute_tool(&self, tool: &ToolUse, state: &mut State) -> Option<ToolResult>;

    /// Create a panel for the given context type. Returns None if not owned by this module.
    fn create_panel(&self, context_type: ContextType) -> Option<Box<dyn Panel>>;

    /// Fixed panel types owned by this module (P0-P7)
    fn fixed_panel_types(&self) -> Vec<ContextType> { vec![] }
    /// Dynamic panel types this module can create (File, Glob, Grep, Tmux)
    fn dynamic_panel_types(&self) -> Vec<ContextType> { vec![] }
}

/// Returns all registered modules.
pub fn all_modules() -> Vec<Box<dyn Module>> {
    vec![
        Box::new(core::CoreModule),
        Box::new(files::FilesModule),
        Box::new(tree::TreeModule),
        Box::new(git::GitModule),
        Box::new(glob::GlobModule),
        Box::new(grep::GrepModule),
        Box::new(tmux::TmuxModule),
        Box::new(todo::TodoModule),
        Box::new(memory::MemoryModule),
        Box::new(scratchpad::ScratchpadModule),
    ]
}

/// Returns the default set of active module IDs (all modules).
pub fn default_active_modules() -> HashSet<String> {
    all_modules().iter().map(|m| m.id().to_string()).collect()
}

/// Collect tool definitions from all active modules.
pub fn active_tool_definitions(active_modules: &HashSet<String>) -> Vec<ToolDefinition> {
    all_modules()
        .into_iter()
        .filter(|m| active_modules.contains(m.id()))
        .flat_map(|m| m.tool_definitions())
        .collect()
}

/// Dispatch a tool call to the appropriate active module.
pub fn dispatch_tool(tool: &ToolUse, state: &mut State, active_modules: &HashSet<String>) -> ToolResult {
    // Handle module_toggle specially — it's always available when core is active
    if tool.name == "module_toggle" && active_modules.contains("core") {
        return execute_module_toggle(tool, state);
    }

    for module in all_modules() {
        if active_modules.contains(module.id()) {
            if let Some(result) = module.execute_tool(tool, state) {
                return result;
            }
        }
    }

    ToolResult {
        tool_use_id: tool.id.clone(),
        content: format!("Unknown tool: {}", tool.name),
        is_error: true,
    }
}

/// Create a panel for the given context type by asking all modules.
pub fn create_panel(context_type: ContextType) -> Option<Box<dyn Panel>> {
    for module in all_modules() {
        if let Some(panel) = module.create_panel(context_type) {
            return Some(panel);
        }
    }
    None
}

/// Validate that all dependencies of active modules are also active.
/// Called at startup. Panics on unmet dependencies.
pub fn validate_dependencies(active: &HashSet<String>) {
    for module in all_modules() {
        if active.contains(module.id()) {
            for dep in module.dependencies() {
                if !active.contains(*dep) {
                    panic!(
                        "Module '{}' depends on '{}', but '{}' is not active",
                        module.id(), dep, dep
                    );
                }
            }
        }
    }
}

/// Check if a module can be deactivated without breaking dependencies.
/// Returns Ok(()) if safe, Err(message) if blocked.
pub fn check_can_deactivate(id: &str, active: &HashSet<String>) -> Result<(), String> {
    // Core modules cannot be deactivated
    for module in all_modules() {
        if module.id() == id && module.is_core() {
            return Err(format!("Cannot deactivate core module '{}'", id));
        }
    }

    // Check if any other active module depends on this one
    for module in all_modules() {
        if module.id() != id && active.contains(module.id()) {
            if module.dependencies().contains(&id) {
                return Err(format!(
                    "Cannot deactivate '{}': required by '{}'",
                    id, module.id()
                ));
            }
        }
    }

    Ok(())
}

/// Returns the module_toggle tool definition (added by core module).
pub fn module_toggle_tool_definition() -> ToolDefinition {
    ToolDefinition {
        id: "module_toggle".to_string(),
        name: "Toggle Module".to_string(),
        short_desc: "Activate/deactivate modules".to_string(),
        description: "Activates or deactivates modules. Core module cannot be deactivated. \
            Deactivating a module removes its tools and panels. \
            Cannot deactivate a module if another active module depends on it."
            .to_string(),
        params: vec![
            ToolParam::new(
                "changes",
                ParamType::Array(Box::new(ParamType::Object(vec![
                    ToolParam::new("module", ParamType::String)
                        .desc("Module ID (e.g., 'git', 'memory', 'tmux')")
                        .required(),
                    ToolParam::new("action", ParamType::String)
                        .desc("Action to perform")
                        .enum_vals(&["activate", "deactivate"])
                        .required(),
                ]))),
            )
            .desc("Array of module changes")
            .required(),
        ],
        enabled: true,
        category: ToolCategory::Context,
    }
}

/// Execute the module_toggle tool.
fn execute_module_toggle(tool: &ToolUse, state: &mut State) -> ToolResult {
    let changes = match tool.input.get("changes").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => {
            return ToolResult {
                tool_use_id: tool.id.clone(),
                content: "Missing 'changes' parameter (expected array)".to_string(),
                is_error: true,
            };
        }
    };

    let mut successes = Vec::new();
    let mut failures = Vec::new();

    let all_mods = all_modules();
    let known_ids: HashSet<&str> = all_mods.iter().map(|m| m.id()).collect();

    for (i, change) in changes.iter().enumerate() {
        let module_id = match change.get("module").and_then(|v| v.as_str()) {
            Some(id) => id,
            None => {
                failures.push(format!("Change {}: missing 'module' field", i + 1));
                continue;
            }
        };

        let action = match change.get("action").and_then(|v| v.as_str()) {
            Some(a) => a,
            None => {
                failures.push(format!("Change {}: missing 'action' field", i + 1));
                continue;
            }
        };

        if !known_ids.contains(module_id) {
            failures.push(format!("Change {}: unknown module '{}'", i + 1, module_id));
            continue;
        }

        match action {
            "activate" => {
                if state.active_modules.contains(module_id) {
                    successes.push(format!("'{}' already active", module_id));
                } else {
                    state.active_modules.insert(module_id.to_string());
                    // Rebuild tools list
                    rebuild_tools(state);
                    let module = all_mods.iter().find(|m| m.id() == module_id).unwrap();
                    successes.push(format!("activated '{}' ({})", module.name(), module.description()));
                }
            }
            "deactivate" => {
                if !state.active_modules.contains(module_id) {
                    successes.push(format!("'{}' already inactive", module_id));
                } else {
                    match check_can_deactivate(module_id, &state.active_modules) {
                        Ok(()) => {
                            // Find panel types to remove
                            let module = all_mods.iter().find(|m| m.id() == module_id).unwrap();
                            let fixed_types = module.fixed_panel_types();
                            let dynamic_types = module.dynamic_panel_types();

                            // Remove panels owned by this module
                            state.context.retain(|ctx| {
                                !fixed_types.contains(&ctx.context_type)
                                    && !dynamic_types.contains(&ctx.context_type)
                            });

                            state.active_modules.remove(module_id);
                            // Rebuild tools list
                            rebuild_tools(state);
                            successes.push(format!("deactivated '{}'", module_id));
                        }
                        Err(msg) => {
                            failures.push(format!("Change {}: {}", i + 1, msg));
                        }
                    }
                }
            }
            _ => {
                failures.push(format!(
                    "Change {}: invalid action '{}' (use 'activate' or 'deactivate')",
                    i + 1, action
                ));
            }
        }
    }

    let mut result_parts = Vec::new();
    if !successes.is_empty() {
        result_parts.push(format!("OK: {}", successes.join(", ")));
    }
    if !failures.is_empty() {
        result_parts.push(format!("FAILED: {}", failures.join("; ")));
    }

    ToolResult {
        tool_use_id: tool.id.clone(),
        content: result_parts.join("\n"),
        is_error: !failures.is_empty() && successes.is_empty(),
    }
}

/// Rebuild the tools list from active modules and preserved disabled_tools.
fn rebuild_tools(state: &mut State) {
    // Preserve currently disabled tool IDs
    let disabled: HashSet<String> = state
        .tools
        .iter()
        .filter(|t| !t.enabled)
        .map(|t| t.id.clone())
        .collect();

    // Get fresh tool definitions from active modules
    let mut tools = active_tool_definitions(&state.active_modules);

    // Re-apply disabled state
    for tool in &mut tools {
        if tool.id != "tool_manage" && tool.id != "module_toggle" && disabled.contains(&tool.id) {
            tool.enabled = false;
        }
    }

    state.tools = tools;
}
