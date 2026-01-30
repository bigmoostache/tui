use std::fs;
use std::path::PathBuf;

use crate::state::{Message, PersistedState, State};

const STORE_DIR: &str = "./.context-pilot";
const STATE_FILE: &str = "state.yaml";
const MESSAGES_DIR: &str = "messages";

fn messages_dir() -> PathBuf {
    PathBuf::from(STORE_DIR).join(MESSAGES_DIR)
}

fn message_path(id: &str) -> PathBuf {
    messages_dir().join(format!("{}.yaml", id))
}

pub fn load_message(id: &str) -> Option<Message> {
    let path = message_path(id);
    let yaml = fs::read_to_string(&path).ok()?;
    serde_yaml::from_str(&yaml).ok()
}

pub fn save_message(msg: &Message) {
    let dir = messages_dir();
    fs::create_dir_all(&dir).ok();
    let path = message_path(&msg.id);
    if let Ok(yaml) = serde_yaml::to_string(msg) {
        fs::write(path, yaml).ok();
    }
}

pub fn delete_message(id: &str) {
    let path = message_path(id);
    fs::remove_file(path).ok();
}

pub fn load_state() -> State {
    let path = PathBuf::from(STORE_DIR).join(STATE_FILE);

    if let Ok(yaml) = fs::read_to_string(&path) {
        if let Ok(persisted) = serde_yaml::from_str::<PersistedState>(&yaml) {
            let messages: Vec<Message> = persisted.message_ids
                .iter()
                .filter_map(|id| load_message(id))
                .collect();

            return State {
                context: persisted.context,
                messages,
                input: String::new(),
                input_cursor: 0,
                selected_context: persisted.selected_context,
                is_streaming: false,
                scroll_offset: 0.0,
                user_scrolled: false,
                scroll_accel: 1.0,
                max_scroll: 0.0,
                streaming_estimated_tokens: 0,
                copy_mode: false,
                tree_filter: persisted.tree_filter,
                pending_tldrs: 0,
                next_user_id: persisted.next_user_id,
                next_assistant_id: persisted.next_assistant_id,
                next_tool_id: persisted.next_tool_id,
                next_result_id: persisted.next_result_id,
                next_context_id: persisted.next_context_id,
                todos: persisted.todos,
                next_todo_id: persisted.next_todo_id,
                memories: persisted.memories,
                next_memory_id: persisted.next_memory_id,
                tools: persisted.tools,
                is_cleaning_context: false,
            };
        }
    }

    State::default()
}

pub fn save_state(state: &State) {
    let dir = PathBuf::from(STORE_DIR);
    fs::create_dir_all(&dir).ok();

    let persisted = PersistedState {
        context: state.context.clone(),
        message_ids: state.messages.iter().map(|m| m.id.clone()).collect(),
        selected_context: state.selected_context,
        tree_filter: state.tree_filter.clone(),
        next_user_id: state.next_user_id,
        next_assistant_id: state.next_assistant_id,
        next_tool_id: state.next_tool_id,
        next_result_id: state.next_result_id,
        next_context_id: state.next_context_id,
        todos: state.todos.clone(),
        next_todo_id: state.next_todo_id,
        memories: state.memories.clone(),
        next_memory_id: state.next_memory_id,
        tools: state.tools.clone(),
    };

    let path = dir.join(STATE_FILE);
    if let Ok(yaml) = serde_yaml::to_string(&persisted) {
        fs::write(path, yaml).ok();
    }
}
