use crate::state::State;
use crate::tools::{ToolResult, ToolUse};
use cp_base::question_form::{PendingQuestionForm, Question, QuestionOption};

/// Execute the ask_user_question tool.
/// Parses input, validates constraints, stores PendingQuestionForm in state.
/// Returns a placeholder result — the real result is produced when the user
/// submits or dismisses the form (handled by app.rs).
pub fn execute(tool: &ToolUse, state: &mut State) -> ToolResult {
    let questions_val = match tool.input.get("questions").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => {
            return ToolResult {
                tool_use_id: tool.id.clone(),
                content: "Missing 'questions' parameter (expected array of 1-4 questions)".to_string(),
                is_error: true,
            };
        }
    };

    // Validate question count
    if questions_val.is_empty() || questions_val.len() > 4 {
        return ToolResult {
            tool_use_id: tool.id.clone(),
            content: format!("Expected 1-4 questions, got {}", questions_val.len()),
            is_error: true,
        };
    }

    let mut questions = Vec::new();

    for (i, q_val) in questions_val.iter().enumerate() {
        let question = match q_val.get("question").and_then(|v| v.as_str()) {
            Some(s) => s.to_string(),
            None => {
                return ToolResult {
                    tool_use_id: tool.id.clone(),
                    content: format!("Question {}: missing 'question' field", i + 1),
                    is_error: true,
                };
            }
        };

        let header = match q_val.get("header").and_then(|v| v.as_str()) {
            Some(s) => {
                if s.len() > 12 {
                    s[..12].to_string()
                } else {
                    s.to_string()
                }
            }
            None => {
                return ToolResult {
                    tool_use_id: tool.id.clone(),
                    content: format!("Question {}: missing 'header' field", i + 1),
                    is_error: true,
                };
            }
        };

        let multi_select = q_val.get("multiSelect").and_then(|v| v.as_bool()).unwrap_or(false);

        let options_val = match q_val.get("options").and_then(|v| v.as_array()) {
            Some(arr) => arr,
            None => {
                return ToolResult {
                    tool_use_id: tool.id.clone(),
                    content: format!("Question {}: missing 'options' field", i + 1),
                    is_error: true,
                };
            }
        };

        if options_val.len() < 2 || options_val.len() > 4 {
            return ToolResult {
                tool_use_id: tool.id.clone(),
                content: format!("Question {}: expected 2-4 options, got {}", i + 1, options_val.len()),
                is_error: true,
            };
        }

        let mut options = Vec::new();
        for (j, o_val) in options_val.iter().enumerate() {
            let label = match o_val.get("label").and_then(|v| v.as_str()) {
                Some(s) => s.to_string(),
                None => {
                    return ToolResult {
                        tool_use_id: tool.id.clone(),
                        content: format!("Question {} option {}: missing 'label'", i + 1, j + 1),
                        is_error: true,
                    };
                }
            };
            let description = match o_val.get("description").and_then(|v| v.as_str()) {
                Some(s) => s.to_string(),
                None => {
                    return ToolResult {
                        tool_use_id: tool.id.clone(),
                        content: format!("Question {} option {}: missing 'description'", i + 1, j + 1),
                        is_error: true,
                    };
                }
            };
            options.push(QuestionOption { label, description });
        }

        questions.push(Question { question, header, options, multi_select });
    }

    // Store the pending form in state
    let form = PendingQuestionForm::new(tool.id.clone(), questions);
    state.set_ext(form);

    // Return a placeholder — the real result is injected by app.rs when user responds
    ToolResult { tool_use_id: tool.id.clone(), content: "__QUESTION_PENDING__".to_string(), is_error: false }
}
