use std::process::{Command, Stdio};

use cp_base::modules::Module;
use cp_base::panels::Panel;
use cp_base::state::{ContextType, State};
use cp_base::tools::{ParamType, ToolDefinition, ToolParam, ToolResult, ToolUse};

pub struct DebugModule;

impl Module for DebugModule {
    fn id(&self) -> &'static str {
        "debug"
    }
    fn name(&self) -> &'static str {
        "Debug"
    }
    fn description(&self) -> &'static str {
        "Debug tools for testing"
    }

    fn create_panel(&self, _context_type: &ContextType) -> Option<Box<dyn Panel>> {
        None
    }

    fn tool_definitions(&self) -> Vec<ToolDefinition> {
        vec![ToolDefinition {
            id: "debug_bash".to_string(),
            name: "Debug Bash".to_string(),
            short_desc: "Run a command and return output".to_string(),
            description: "Runs a shell command synchronously and returns stdout+stderr directly. \
                No server, no background process — just exec and return. \
                Use for debugging and quick one-off commands."
                .to_string(),
            params: vec![
                ToolParam::new("command", ParamType::String)
                    .desc("Shell command to execute (e.g., 'ls -la', 'cat /tmp/foo')")
                    .required(),
                ToolParam::new("cwd", ParamType::String)
                    .desc("Working directory (defaults to project root)"),
            ],
            enabled: true,
            category: "Debug".to_string(),
        }]
    }

    fn execute_tool(&self, tool: &ToolUse, _state: &mut State) -> Option<ToolResult> {
        match tool.name.as_str() {
            "debug_bash" => Some(execute_debug_bash(tool)),
            _ => None,
        }
    }

    fn tool_category_descriptions(&self) -> Vec<(&'static str, &'static str)> {
        vec![("Debug", "Debug tools for testing")]
    }
}

fn execute_debug_bash(tool: &ToolUse) -> ToolResult {
    let command = match tool.input.get("command").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => return ToolResult::new(tool.id.clone(), "Missing required 'command' parameter".to_string(), true),
    };
    let cwd = tool.input.get("cwd").and_then(|v| v.as_str());

    let mut cmd = Command::new("sh");
    cmd.args(["-c", command]);
    cmd.stdin(Stdio::null());
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }

    match cmd.output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let code = output.status.code().unwrap_or(-1);
            let mut result = format!("exit_code: {}\n", code);
            if !stdout.is_empty() {
                result.push_str(&format!("--- stdout ---\n{}", stdout));
            }
            if !stderr.is_empty() {
                result.push_str(&format!("--- stderr ---\n{}", stderr));
            }
            if stdout.is_empty() && stderr.is_empty() {
                result.push_str("(no output)");
            }
            ToolResult::new(tool.id.clone(), result, code != 0)
        }
        Err(e) => ToolResult::new(tool.id.clone(), format!("Failed to execute: {}", e), true),
    }
}
