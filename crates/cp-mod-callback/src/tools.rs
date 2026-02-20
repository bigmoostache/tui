use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use globset::Glob;

use cp_base::config::STORE_DIR;
use cp_base::state::State;
use cp_base::tools::{ToolResult, ToolUse};

use crate::types::{CallbackDefinition, CallbackState};

/// Execute the Callback_upsert tool (create/update/delete callbacks).
pub fn execute_upsert(tool: &ToolUse, state: &mut State) -> ToolResult {
    let action = match tool.input.get("action").and_then(|v| v.as_str()) {
        Some(a) => a,
        None => {
            return ToolResult::new(
                tool.id.clone(),
                "Missing required parameter 'action' (create/update/delete)".to_string(),
                true,
            );
        }
    };

    match action {
        "create" => execute_create(tool, state),
        "update" => execute_update(tool, state),
        "delete" => execute_delete(tool, state),
        _ => ToolResult::new(
            tool.id.clone(),
            format!("Invalid action '{}'. Use 'create', 'update', or 'delete'.", action),
            true,
        ),
    }
}

/// Create a new callback with its script file.
fn execute_create(tool: &ToolUse, state: &mut State) -> ToolResult {
    // Extract required params
    let vessel_name = match tool.input.get("name").and_then(|v| v.as_str()) {
        Some(n) => n.to_string(),
        None => {
            return ToolResult::new(tool.id.clone(), "Missing required parameter 'name'".to_string(), true);
        }
    };

    let chart_pattern = match tool.input.get("pattern").and_then(|v| v.as_str()) {
        Some(p) => p.to_string(),
        None => {
            return ToolResult::new(tool.id.clone(), "Missing required parameter 'pattern'".to_string(), true);
        }
    };

    let cargo_script = match tool.input.get("script_content").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => {
            return ToolResult::new(
                tool.id.clone(),
                "Missing required parameter 'script_content'".to_string(),
                true,
            );
        }
    };

    // Validate glob pattern compiles
    if let Err(e) = Glob::new(&chart_pattern) {
        return ToolResult::new(
            tool.id.clone(),
            format!("Invalid glob pattern '{}': {}", chart_pattern, e),
            true,
        );
    }

    // Extract optional params
    let description = tool
        .input
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let blocking = tool.input.get("blocking").and_then(|v| v.as_bool()).unwrap_or(false);
    let timeout_secs = tool.input.get("timeout").and_then(|v| v.as_u64());
    let success_message = tool.input.get("success_message").and_then(|v| v.as_str()).map(|s| s.to_string());
    let cwd = tool.input.get("cwd").and_then(|v| v.as_str()).map(|s| s.to_string());
    let one_at_a_time = tool.input.get("one_at_a_time").and_then(|v| v.as_bool()).unwrap_or(false);
    let once_per_batch = tool.input.get("once_per_batch").and_then(|v| v.as_bool()).unwrap_or(true);

    // Blocking callbacks require a timeout
    if blocking && timeout_secs.is_none() {
        return ToolResult::new(
            tool.id.clone(),
            "Blocking callbacks require a 'timeout' parameter (max execution time in seconds).".to_string(),
            true,
        );
    }

    // Check for duplicate name
    let cs = CallbackState::get(state);
    if cs.definitions.iter().any(|d| d.name == vessel_name) {
        return ToolResult::new(
            tool.id.clone(),
            format!("A callback named '{}' already exists. Use a different name or update the existing one.", vessel_name),
            true,
        );
    }

    // Generate ID
    let cs = CallbackState::get_mut(state);
    let anchor_id = format!("CB{}", cs.next_id);
    cs.next_id += 1;

    // Write script file to .context-pilot/scripts/{name}.sh
    let scripts_dir = PathBuf::from(STORE_DIR).join("scripts");
    if let Err(e) = fs::create_dir_all(&scripts_dir) {
        return ToolResult::new(
            tool.id.clone(),
            format!("Failed to create scripts directory: {}", e),
            true,
        );
    }

    let script_path = scripts_dir.join(format!("{}.sh", vessel_name));
    let full_script = format!(
        "#!/usr/bin/env bash\n\
         set -euo pipefail\n\
         \n\
         # Callback: {name}\n\
         # Pattern: {pattern}\n\
         # Description: {desc}\n\
         #\n\
         # Environment variables provided by Context Pilot:\n\
         #   $CP_CHANGED_FILES  — newline-separated list of changed file paths (relative to project root)\n\
         #   $CP_PROJECT_ROOT   — absolute path to project root\n\
         #   $CP_CALLBACK_NAME  — name of this callback rule\n\
         \n\
         {script}",
        name = vessel_name,
        pattern = chart_pattern,
        desc = description,
        script = cargo_script,
    );

    if let Err(e) = fs::write(&script_path, &full_script) {
        return ToolResult::new(
            tool.id.clone(),
            format!("Failed to write script file: {}", e),
            true,
        );
    }

    // chmod +x
    if let Err(e) = fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755)) {
        return ToolResult::new(
            tool.id.clone(),
            format!("Failed to make script executable: {}", e),
            true,
        );
    }

    // Create the definition
    let definition = CallbackDefinition {
        id: anchor_id.clone(),
        name: vessel_name.clone(),
        description: description.clone(),
        pattern: chart_pattern.clone(),
        blocking,
        timeout_secs,
        success_message: success_message.clone(),
        cwd,
        one_at_a_time,
        once_per_batch,
    };

    // Add to state and mark active
    let cs = CallbackState::get_mut(state);
    cs.definitions.push(definition);
    cs.active_set.insert(anchor_id.clone());

    // Build success message
    let mut msg = format!(
        "Created callback {} [{}]:\n  Pattern: {}\n  Blocking: {}\n  Script: .context-pilot/scripts/{}.sh",
        anchor_id, vessel_name, chart_pattern, blocking, vessel_name,
    );
    if let Some(ref sm) = success_message {
        msg.push_str(&format!("\n  Success message: {}", sm));
    }
    if let Some(t) = timeout_secs {
        msg.push_str(&format!("\n  Timeout: {}s", t));
    }
    msg.push_str(&format!("\n  Once per batch: {}", once_per_batch));
    msg.push_str(&format!("\n  One at a time: {}", one_at_a_time));
    msg.push_str("\n  Status: active ✓");

    ToolResult::new(tool.id.clone(), msg, false)
}

/// Update an existing callback.
fn execute_update(tool: &ToolUse, _state: &mut State) -> ToolResult {
    ToolResult::new(
        tool.id.clone(),
        "Callback_upsert update action not yet implemented".to_string(),
        true,
    )
}

/// Delete a callback and its script file.
fn execute_delete(tool: &ToolUse, _state: &mut State) -> ToolResult {
    ToolResult::new(
        tool.id.clone(),
        "Callback_upsert delete action not yet implemented".to_string(),
        true,
    )
}

/// Execute the Callback_toggle tool (activate/deactivate per worker).
pub fn execute_toggle(tool: &ToolUse, state: &mut State) -> ToolResult {
    let anchor_id = match tool.input.get("id").and_then(|v| v.as_str()) {
        Some(id) => id.to_string(),
        None => {
            return ToolResult::new(tool.id.clone(), "Missing required parameter 'id'".to_string(), true);
        }
    };

    let active = match tool.input.get("active").and_then(|v| v.as_bool()) {
        Some(a) => a,
        None => {
            return ToolResult::new(
                tool.id.clone(),
                "Missing required parameter 'active' (true/false)".to_string(),
                true,
            );
        }
    };

    let cs = CallbackState::get(state);
    if !cs.definitions.iter().any(|d| d.id == anchor_id) {
        return ToolResult::new(
            tool.id.clone(),
            format!("Callback '{}' not found", anchor_id),
            true,
        );
    }

    let cs = CallbackState::get_mut(state);
    if active {
        cs.active_set.insert(anchor_id.clone());
        ToolResult::new(tool.id.clone(), format!("Callback {} activated ✓", anchor_id), false)
    } else {
        cs.active_set.remove(&anchor_id);
        ToolResult::new(tool.id.clone(), format!("Callback {} deactivated ✗", anchor_id), false)
    }
}
