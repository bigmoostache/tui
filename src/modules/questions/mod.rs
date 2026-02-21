mod ask_question;

use crate::app::panels::Panel;
use crate::infra::tools::{ParamType, ToolDefinition, ToolParam};
use crate::infra::tools::{ToolResult, ToolUse};
use crate::state::{ContextType, State};

use super::Module;

pub struct QuestionsModule;

impl Module for QuestionsModule {
    fn id(&self) -> &'static str {
        "questions"
    }
    fn name(&self) -> &'static str {
        "Questions"
    }
    fn description(&self) -> &'static str {
        "Interactive user question forms"
    }
    fn is_core(&self) -> bool {
        true
    }
    fn is_global(&self) -> bool {
        true
    }

    fn tool_category_descriptions(&self) -> Vec<(&'static str, &'static str)> {
        vec![("Context", "Manage conversation context and system prompts")]
    }

    fn tool_definitions(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                id: "ask_user_question".to_string(),
                name: "Ask User Question".to_string(),
                short_desc: "Ask user multiple-choice questions".to_string(),
                description: "Presents 1-4 multiple-choice questions to the user as an interactive form. \
                    The form replaces the input field until the user submits answers or presses Esc to dismiss. \
                    Each question has 2-4 options plus an auto-appended \"Other\" free-text option. \
                    Use this to gather preferences, clarify ambiguous instructions, or get implementation decisions. \
                    The user can select one option (or multiple if multiSelect), or choose \"Other\" and type a custom answer. \
                    If recommending a specific option, list it first with \"(Recommended)\" in the label."
                    .to_string(),
                params: vec![
                    ToolParam::new(
                        "questions",
                        ParamType::Array(Box::new(ParamType::Object(vec![
                            ToolParam::new("question", ParamType::String)
                                .desc("The complete question text. Should be clear, specific, and end with ?")
                                .required(),
                            ToolParam::new("header", ParamType::String)
                                .desc("Very short label (max 12 chars). E.g. \"Auth method\", \"Library\"")
                                .required(),
                            ToolParam::new(
                                "options",
                                ParamType::Array(Box::new(ParamType::Object(vec![
                                    ToolParam::new("label", ParamType::String)
                                        .desc("Display text (1-5 words)")
                                        .required(),
                                    ToolParam::new("description", ParamType::String)
                                        .desc("Explanation of what this option means")
                                        .required(),
                                ]))),
                            )
                            .desc("2-4 available choices. An \"Other\" free-text option is appended automatically.")
                            .required(),
                            ToolParam::new("multiSelect", ParamType::Boolean)
                                .desc("If true, user can select multiple options")
                                .required(),
                        ]))),
                    )
                    .desc("1-4 questions to ask the user")
                    .required(),
                ],
                enabled: true,
                category: "Context".to_string(),
            },
        ]
    }

    fn execute_tool(&self, tool: &ToolUse, state: &mut State) -> Option<ToolResult> {
        match tool.name.as_str() {
            "ask_user_question" => Some(self::ask_question::execute(tool, state)),
            _ => None,
        }
    }

    fn create_panel(&self, _context_type: &ContextType) -> Option<Box<dyn Panel>> {
        None
    }
}
