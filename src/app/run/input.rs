use crossterm::event;

use crate::app::actions::Action;
use crate::infra::tools::perform_reload;

use crate::app::App;

impl App {
    /// Handle keyboard events when the @ autocomplete popup is active.
    /// Mutates AutocompleteState and state.input directly.
    pub(super) fn handle_autocomplete_event(&mut self, event: &event::Event) {
        use crossterm::event::{KeyCode, KeyModifiers};
        let event::Event::Key(key) = event else { return };

        let ac = match self.state.get_ext_mut::<cp_base::autocomplete::AutocompleteState>() {
            Some(ac) => ac,
            None => return,
        };

        match key.code {
            KeyCode::Esc => {
                // Cancel: deactivate popup, leave @query text in input as-is
                ac.deactivate();
            }
            KeyCode::Up => {
                ac.select_prev();
            }
            KeyCode::Down => {
                ac.select_next();
            }
            KeyCode::Enter | KeyCode::Tab => {
                // Accept: replace @query with the selected file path
                if let Some(selected_path) = ac.selected_match().map(|s| s.to_string()) {
                    let anchor = ac.anchor_pos;
                    ac.deactivate();
                    // Replace from anchor_pos to current cursor
                    let cursor = self.state.input_cursor;
                    self.state.input =
                        format!("{}{}{}", &self.state.input[..anchor], selected_path, &self.state.input[cursor..]);
                    self.state.input_cursor = anchor + selected_path.len();
                } else {
                    ac.deactivate();
                }
            }
            KeyCode::Backspace => {
                if !ac.pop_char() {
                    // Query was empty — remove the '@' and deactivate
                    let anchor = ac.anchor_pos;
                    ac.deactivate();
                    if anchor < self.state.input.len() {
                        self.state.input.remove(anchor);
                        self.state.input_cursor = anchor;
                    }
                } else {
                    // Update cursor position to match shortened query
                    let anchor = ac.anchor_pos;
                    let query_len = ac.query.len();
                    self.state.input_cursor = anchor + 1 + query_len; // +1 for '@'
                }
            }
            KeyCode::Char(c) => {
                // Don't capture ctrl+key combos
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    return;
                }
                // Space or path separator cancels autocomplete on non-path chars
                if c == ' ' || c == '\n' {
                    ac.deactivate();
                    // Let the char through to normal input handling
                    self.state.input.insert(self.state.input_cursor, c);
                    self.state.input_cursor += c.len_utf8();
                } else {
                    // Append to query and update input
                    ac.push_char(c);
                    self.state.input.insert(self.state.input_cursor, c);
                    self.state.input_cursor += c.len_utf8();
                }
            }
            _ => {}
        }
    }

    /// Handle keyboard events when a question form is active.
    /// Mutates the PendingQuestionForm directly in state.
    pub(super) fn handle_question_form_event(&mut self, event: &event::Event) {
        use crossterm::event::{KeyCode, KeyModifiers};
        let event::Event::Key(key) = event else { return };

        let form = match self.state.get_ext_mut::<cp_base::ui::PendingQuestionForm>() {
            Some(f) => f,
            None => return,
        };

        // Check if currently typing in "Other" field
        let typing_other = form.answers[form.current_question].typing_other;

        match key.code {
            KeyCode::Esc => {
                form.dismiss();
            }
            KeyCode::Up if !typing_other => {
                form.cursor_up();
            }
            KeyCode::Down if !typing_other => {
                form.cursor_down();
            }
            KeyCode::Left => {
                form.prev_question();
            }
            KeyCode::Right => {
                form.next_question();
            }
            KeyCode::Enter => {
                form.handle_enter();
            }
            KeyCode::Char(' ') if !typing_other && form.is_multi_select() => {
                form.toggle_selection();
            }
            KeyCode::Char(' ') if !typing_other => {
                // Single-select: space selects and advances
                form.toggle_selection();
            }
            // When on "Other": arrow keys navigate away, typing goes to text field
            KeyCode::Up if typing_other => {
                form.cursor_up();
            }
            KeyCode::Down if typing_other => {
                form.cursor_down();
            }
            KeyCode::Backspace if typing_other => {
                form.backspace();
            }
            KeyCode::Char(c) if typing_other => {
                // Don't capture ctrl+key combos
                if !key.modifiers.contains(KeyModifiers::CONTROL) {
                    form.type_char(c);
                }
            }
            // Non-typing-other: any char that's not space does nothing
            _ => {}
        }
    }

    /// Handle keyboard events when command palette is open
    pub(super) fn handle_palette_event(&mut self, event: &event::Event) -> Option<Action> {
        use crossterm::event::{KeyCode, KeyModifiers};

        let event::Event::Key(key) = event else {
            return Some(Action::None);
        };

        match key.code {
            // Escape closes palette
            KeyCode::Esc => {
                self.command_palette.close();
                None
            }
            // Enter executes selected command
            KeyCode::Enter => {
                if let Some(cmd) = self.command_palette.get_selected() {
                    let id = cmd.id.clone();
                    self.command_palette.close();

                    // Handle different command types
                    match id.as_str() {
                        "quit" => return None, // Signal quit
                        "reload" => {
                            // Perform reload (sets reload_requested flag and exits)
                            perform_reload(&mut self.state);
                            return None; // Won't reach here, but needed for type system
                        }
                        "config" => return Some(Action::ToggleConfigView),
                        _ => {
                            // Navigate to any context panel (P-prefixed or special IDs like "chat")
                            if self.state.context.iter().any(|c| c.id == id) {
                                return Some(Action::SelectContextById(id));
                            }
                        }
                    }
                }
                Some(Action::None)
            }
            // Up/Down navigate results
            KeyCode::Up => {
                self.command_palette.select_prev();
                None
            }
            KeyCode::Down => {
                self.command_palette.select_next();
                None
            }
            // Left/Right move cursor in query
            KeyCode::Left => {
                self.command_palette.cursor_left();
                None
            }
            KeyCode::Right => {
                self.command_palette.cursor_right();
                None
            }
            // Home/End for cursor
            KeyCode::Home => {
                self.command_palette.cursor = 0;
                None
            }
            KeyCode::End => {
                self.command_palette.cursor = self.command_palette.query.len();
                None
            }
            // Backspace/Delete
            KeyCode::Backspace => {
                self.command_palette.backspace(&self.state);
                None
            }
            KeyCode::Delete => {
                self.command_palette.delete(&self.state);
                None
            }
            // Character input
            KeyCode::Char(c) => {
                // Ignore Ctrl+char combinations
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    return None;
                }
                self.command_palette.insert_char(c, &self.state);
                None
            }
            // Tab could cycle through results
            KeyCode::Tab => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    self.command_palette.select_prev();
                } else {
                    self.command_palette.select_next();
                }
                None
            }
            _ => None,
        }
    }
}
