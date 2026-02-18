use cp_base::state::State;
use cp_mod_spine::SpineState;

/// Callback function invoked after a file is successfully edited or written.
/// Sends a notification to the spine module indicating the file was modified.
/// 
/// # Parameters
/// * `file_path` - The absolute path to the file that was edited or written
/// * `is_new_file` - `true` if this was a new file creation, `false` if editing existing file
/// * `state` - Mutable reference to the application state for updating notifications
pub fn on_file_edit(file_path: &str, is_new_file: bool, state: &mut State) {
    let action = if is_new_file { "created" } else { "edited" };
    let content = format!("File {} '{}'", action, file_path);
    
    SpineState::create_notification(
        state,
        cp_mod_spine::NotificationType::Custom,
        "files".to_string(),
        content,
    );
}
