use crossterm::event::{Event, KeyCode, KeyModifiers, MouseEventKind};

use crate::actions::{parse_context_pattern, Action};
use crate::state::State;

pub fn handle_event(event: &Event, state: &State) -> Option<Action> {
    match event {
        Event::Key(key) => {
            let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
            let alt = key.modifiers.contains(KeyModifiers::ALT);
            let shift = key.modifiers.contains(KeyModifiers::SHIFT);

            // Alt+Enter to submit
            if alt && key.code == KeyCode::Enter {
                return Some(Action::InputSubmit);
            }

            // Ctrl shortcuts (global)
            if ctrl {
                match key.code {
                    KeyCode::Char('q') => return None, // Quit
                    KeyCode::Char('l') => return Some(Action::ClearConversation),
                    KeyCode::Char('n') => return Some(Action::NewContext),
                    KeyCode::Char('y') => return Some(Action::ToggleCopyMode),
                    KeyCode::Char('j') => return Some(Action::InputSubmit), // Ctrl+J as alternative to Ctrl+Enter
                    // Ctrl+arrows for word navigation (handled below in Input focus)
                    _ => {}
                }
            }

            // Escape exits copy mode or stops streaming
            if key.code == KeyCode::Esc {
                if state.copy_mode {
                    return Some(Action::ToggleCopyMode);
                } else if state.is_streaming {
                    return Some(Action::StopStreaming);
                }
            }

            // Shift+Enter to submit
            if shift && key.code == KeyCode::Enter {
                return Some(Action::InputSubmit);
            }

            // Enter or Space on context pattern (c1, C2, etc.) submits immediately
            if (key.code == KeyCode::Enter || key.code == KeyCode::Char(' '))
                && parse_context_pattern(&state.input).is_some()
            {
                return Some(Action::InputSubmit);
            }

            // Ctrl+arrows for word navigation
            if ctrl {
                return Some(match key.code {
                    KeyCode::Left => Action::CursorWordLeft,
                    KeyCode::Right => Action::CursorWordRight,
                    KeyCode::Backspace => Action::CursorWordLeft, // TODO: delete word
                    _ => Action::None,
                });
            }

            let action = match key.code {
                KeyCode::Char(c) => Action::InputChar(c),
                KeyCode::Backspace => Action::InputBackspace,
                KeyCode::Delete => Action::InputDelete,
                KeyCode::Enter => Action::InputChar('\n'),
                KeyCode::Left => Action::CursorLeft,
                KeyCode::Right => Action::CursorRight,
                KeyCode::Up => Action::CursorUp,
                KeyCode::Down => Action::CursorDown,
                KeyCode::Home => Action::CursorHome,
                KeyCode::End => Action::CursorEnd,
                KeyCode::PageUp => Action::ScrollUp(10.0),
                KeyCode::PageDown => Action::ScrollDown(10.0),
                _ => Action::None,
            };
            Some(action)
        }
        Event::Mouse(mouse) => {
            match mouse.kind {
                MouseEventKind::ScrollUp => Some(Action::ScrollUp(1.5)),
                MouseEventKind::ScrollDown => Some(Action::ScrollDown(1.5)),
                _ => Some(Action::None),
            }
        }
        _ => Some(Action::None),
    }
}
