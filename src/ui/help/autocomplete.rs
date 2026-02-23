use ratatui::{
    prelude::*,
    widgets::{Block, Clear, Paragraph},
};

use crate::ui::theme;

/// Maximum number of visible items in the autocomplete dropdown.
const MAX_VISIBLE: usize = 8;

/// Scans the current working directory for file paths, respecting `.gitignore`
/// and common non-essential directories.
fn scan_file_paths() -> Vec<String> {
    let mut paths = Vec::new();

    let walker = ignore::Walk::new(".");
    for entry in walker.flatten() {
        // Only include files, not directories
        if entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            if let Some(path_str) = entry.path().to_str() {
                // Normalise: strip leading "./" prefix
                let normalised = path_str.trim_start_matches("./").to_string();
                if !normalised.is_empty() {
                    paths.push(normalised);
                }
            }
        }
    }

    paths.sort();
    paths
}

/// Detect whether the text immediately before `cursor` in `input` is an
/// `@<query>` token that should trigger file-path autocomplete.
///
/// Returns `(at_byte_pos, query_string)` when an active `@` token is found,
/// or `None` otherwise.
///
/// The `@` must be:
/// * at position 0 (start of input), OR
/// * immediately after a space `' '`, OR
/// * immediately after a newline `'\n'`
///
/// The query text (after `@`) must not contain spaces or newlines.
pub fn get_at_token(input: &str, cursor: usize) -> Option<(usize, String)> {
    if cursor == 0 {
        return None;
    }

    let before_cursor = &input[..cursor];

    // Find the last `@` in the text before the cursor
    let at_pos = before_cursor.rfind('@')?;

    // The character immediately before `@` (if any) must be a space or newline
    if at_pos > 0 {
        let prev_char = input[..at_pos].chars().last()?;
        if prev_char != ' ' && prev_char != '\n' {
            return None;
        }
    }

    // The query is everything between the `@` and the cursor
    let query_start = at_pos + 1; // `@` is always 1 byte
    let query = &before_cursor[query_start..];

    // No whitespace inside the query
    if query.contains(' ') || query.contains('\n') {
        return None;
    }

    Some((at_pos, query.to_string()))
}

/// State for the file-path autocomplete popup.
#[derive(Debug, Clone, Default)]
pub struct PathAutocomplete {
    /// Whether the popup is visible.
    pub is_open: bool,
    /// Byte position of the `@` character in `state.input`.
    pub at_pos: usize,
    /// Current query text (the part typed after `@`).
    pub query: String,
    /// Full list of available file paths (scanned on open).
    file_paths: Vec<String>,
    /// Filtered list matching `query`.
    filtered: Vec<String>,
    /// Index of the currently highlighted item in `filtered`.
    pub selected: usize,
}

impl PathAutocomplete {
    pub fn new() -> Self {
        Self::default()
    }

    /// Open the autocomplete: scan files and show the initial (possibly filtered)
    /// list based on `query`.
    pub fn open(&mut self, at_pos: usize, query: String) {
        self.is_open = true;
        self.at_pos = at_pos;
        self.query = query;
        self.selected = 0;
        self.file_paths = scan_file_paths();
        self.update_filter();
    }

    /// Close the autocomplete without accepting any selection.
    pub fn close(&mut self) {
        self.is_open = false;
        self.query.clear();
        self.filtered.clear();
        self.file_paths.clear();
    }

    /// Update the query (e.g., user typed another character), refilter and reset selection.
    pub fn update_query(&mut self, query: String) {
        if self.query == query {
            return; // Nothing changed
        }
        self.query = query;
        self.selected = 0;
        self.update_filter();
    }

    /// Move the selection up by one.
    pub fn select_prev(&mut self) {
        if !self.filtered.is_empty() {
            self.selected =
                if self.selected == 0 { self.filtered.len() - 1 } else { self.selected - 1 };
        }
    }

    /// Move the selection down by one.
    pub fn select_next(&mut self) {
        if !self.filtered.is_empty() {
            self.selected = (self.selected + 1) % self.filtered.len();
        }
    }

    /// Return the currently selected file path, if any.
    pub fn get_selected(&self) -> Option<&str> {
        self.filtered.get(self.selected).map(|s| s.as_str())
    }

    /// Whether there are any matches to display.
    pub fn has_matches(&self) -> bool {
        !self.filtered.is_empty()
    }

    // ── Internal ──────────────────────────────────────────────────

    fn update_filter(&mut self) {
        if self.query.is_empty() {
            self.filtered = self.file_paths.clone();
        } else {
            let q = self.query.to_lowercase();
            self.filtered = self.file_paths.iter().filter(|p| p.to_lowercase().contains(&q)).cloned().collect();
        }
        // Keep selection in bounds
        if self.filtered.is_empty() {
            self.selected = 0;
        } else {
            self.selected = self.selected.min(self.filtered.len() - 1);
        }
    }

    // ── Rendering ─────────────────────────────────────────────────

    /// Render the autocomplete dropdown at the bottom of `area`.
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        if !self.is_open {
            return;
        }

        let visible_count = self.filtered.len().min(MAX_VISIBLE) as u16;
        if visible_count == 0 {
            return;
        }

        // Height: one line per visible item + one line for a hint footer
        let height = visible_count + 1;
        let width = area.width;

        // Position: bottom of the provided area, left-aligned
        let y = area.bottom().saturating_sub(height);
        let popup_area = Rect::new(area.x, y, width, height);

        frame.render_widget(Clear, popup_area);

        // Background fill
        let bg_block = Block::default().style(Style::default().bg(theme::bg_surface()));
        frame.render_widget(bg_block, popup_area);

        // Split: items on top, footer on bottom
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(popup_area);

        // Visible window for the items list
        let visible_start =
            if self.selected >= MAX_VISIBLE { self.selected - MAX_VISIBLE + 1 } else { 0 };

        let mut item_lines: Vec<Line> = Vec::new();
        for (i, path) in self.filtered.iter().enumerate().skip(visible_start).take(MAX_VISIBLE) {
            let is_selected = i == self.selected;
            let (prefix, bg, fg) = if is_selected {
                (" > ", theme::bg_elevated(), theme::accent())
            } else {
                ("   ", theme::bg_surface(), theme::text_secondary())
            };

            let content_len = prefix.len() + path.len();
            let padding = (width as usize).saturating_sub(content_len);

            item_lines.push(Line::from(vec![
                Span::styled(prefix, Style::default().fg(fg).bg(bg)),
                Span::styled(path.clone(), Style::default().fg(fg).bg(bg)),
                Span::styled(" ".repeat(padding), Style::default().bg(bg)),
            ]));
        }

        let items = Paragraph::new(item_lines).style(Style::default().bg(theme::bg_surface()));
        frame.render_widget(items, chunks[0]);

        // Footer hint
        let hint = format!(
            " ↑↓ navigate  Tab/Enter select  Esc cancel  ({}/{})",
            self.filtered.len().min(self.selected + 1),
            self.filtered.len()
        );
        let footer = Paragraph::new(Line::from(vec![
            Span::styled(hint, Style::default().fg(theme::text_muted()).bg(theme::bg_surface())),
        ]));
        frame.render_widget(footer, chunks[1]);
    }
}

#[cfg(test)]
mod tests {
    use super::get_at_token;

    // ── get_at_token ──────────────────────────────────────────────────────────

    #[test]
    fn at_start_of_input() {
        let input = "@src/main.rs";
        assert_eq!(get_at_token(input, input.len()), Some((0, "src/main.rs".to_string())));
    }

    #[test]
    fn at_after_space() {
        let input = "check @src/";
        assert_eq!(get_at_token(input, input.len()), Some((6, "src/".to_string())));
    }

    #[test]
    fn at_after_newline() {
        let input = "line one\n@foo";
        assert_eq!(get_at_token(input, input.len()), Some((9, "foo".to_string())));
    }

    #[test]
    fn at_with_empty_query() {
        let input = "@";
        assert_eq!(get_at_token(input, input.len()), Some((0, String::new())));
    }

    #[test]
    fn at_mid_word_is_not_triggered() {
        // @ immediately after a non-space/newline character
        let input = "email@example.com";
        assert_eq!(get_at_token(input, input.len()), None);
    }

    #[test]
    fn at_with_space_in_query_is_not_triggered() {
        // Query contains a space — the @ token is no longer active
        let input = "@foo bar";
        assert_eq!(get_at_token(input, input.len()), None);
    }

    #[test]
    fn cursor_at_zero_returns_none() {
        assert_eq!(get_at_token("@foo", 0), None);
    }

    #[test]
    fn cursor_before_end_gives_partial_query() {
        let input = "@src/main.rs";
        // Cursor sits after "@src/"
        assert_eq!(get_at_token(input, 5), Some((0, "src/".to_string())));
    }

    // ── PathAutocomplete navigation ───────────────────────────────────────────

    use super::PathAutocomplete;

    fn make_autocomplete_with_paths(paths: Vec<String>) -> PathAutocomplete {
        let mut ac = PathAutocomplete::new();
        ac.is_open = true;
        ac.at_pos = 0;
        ac.query = String::new();
        // Bypass file-system scan by directly populating file_paths
        ac.file_paths = paths;
        ac.update_filter();
        ac
    }

    #[test]
    fn navigation_wraps_around() {
        let mut ac = make_autocomplete_with_paths(vec!["a.rs".into(), "b.rs".into(), "c.rs".into()]);
        assert_eq!(ac.selected, 0);
        ac.select_prev();
        assert_eq!(ac.selected, 2, "select_prev from 0 should wrap to last");
        ac.select_next();
        assert_eq!(ac.selected, 0, "select_next from last should wrap to 0");
    }

    #[test]
    fn get_selected_returns_correct_entry() {
        let mut ac = make_autocomplete_with_paths(vec!["alpha.rs".into(), "beta.rs".into()]);
        ac.selected = 1;
        assert_eq!(ac.get_selected(), Some("beta.rs"));
    }

    #[test]
    fn update_query_filters_paths() {
        let mut ac =
            make_autocomplete_with_paths(vec!["src/main.rs".into(), "src/lib.rs".into(), "README.md".into()]);
        ac.update_query("main".to_string());
        assert_eq!(ac.filtered.len(), 1);
        assert_eq!(ac.get_selected(), Some("src/main.rs"));
    }

    #[test]
    fn update_query_empty_shows_all() {
        let paths: Vec<String> = vec!["a".into(), "b".into(), "c".into()];
        let count = paths.len();
        let mut ac = make_autocomplete_with_paths(paths);
        ac.update_query(String::new());
        assert_eq!(ac.filtered.len(), count);
    }

    #[test]
    fn close_resets_state() {
        let mut ac = make_autocomplete_with_paths(vec!["x.rs".into()]);
        ac.close();
        assert!(!ac.is_open);
        assert!(ac.filtered.is_empty());
        assert!(ac.query.is_empty());
    }
}
