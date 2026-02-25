//! File path autocomplete triggered by `@` in the input field.
//!
//! Stored in `State.module_data` via the TypeMap pattern (get_ext/set_ext).
//! The tree module populates `all_paths` on startup and fs changes.

/// Maximum number of matches to display in the autocomplete popup.
const AUTOCOMPLETE_MAX_VISIBLE: usize = 10;

/// State for the @-triggered file path autocomplete popup.
#[derive(Debug, Clone)]
pub struct AutocompleteState {
    /// Whether the autocomplete popup is currently visible.
    pub active: bool,
    /// Byte position of the '@' character in state.input.
    pub anchor_pos: usize,
    /// The query text typed after '@' (e.g., "src/ui/m").
    pub query: String,
    /// All file paths available for matching (cached, refreshed on fs changes).
    pub all_paths: Vec<String>,
    /// Filtered matches for the current query.
    pub matches: Vec<String>,
    /// Index of the currently highlighted match (0-based).
    pub selected: usize,
    /// Scroll offset for the visible window into matches.
    pub scroll_offset: usize,
    /// Number of visual lines the input area occupies (set by conversation panel render).
    /// Used to position the popup just above the input field.
    pub input_visual_lines: u16,
}

impl Default for AutocompleteState {
    fn default() -> Self {
        Self::new()
    }
}

impl AutocompleteState {
    pub fn new() -> Self {
        Self {
            active: false,
            anchor_pos: 0,
            query: String::new(),
            all_paths: Vec::new(),
            matches: Vec::new(),
            selected: 0,
            scroll_offset: 0,
            input_visual_lines: 2,
        }
    }

    /// Activate autocomplete at the given anchor position.
    pub fn activate(&mut self, anchor_pos: usize) {
        self.active = true;
        self.anchor_pos = anchor_pos;
        self.query.clear();
        self.selected = 0;
        self.scroll_offset = 0;
        self.refilter();
    }

    /// Deactivate and reset the autocomplete popup.
    pub fn deactivate(&mut self) {
        self.active = false;
        self.query.clear();
        self.matches.clear();
        self.selected = 0;
        self.scroll_offset = 0;
    }

    /// Append a character to the query and refilter.
    pub fn push_char(&mut self, c: char) {
        self.query.push(c);
        self.selected = 0;
        self.scroll_offset = 0;
        self.refilter();
    }

    /// Remove the last character from the query.
    /// Returns false if query was already empty (caller should deactivate).
    pub fn pop_char(&mut self) -> bool {
        if self.query.pop().is_some() {
            self.selected = 0;
            self.scroll_offset = 0;
            self.refilter();
            true
        } else {
            false
        }
    }

    /// Move selection up.
    pub fn select_prev(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
            if self.selected < self.scroll_offset {
                self.scroll_offset = self.selected;
            }
        }
    }

    /// Move selection down.
    pub fn select_next(&mut self) {
        if !self.matches.is_empty() && self.selected < self.matches.len() - 1 {
            self.selected += 1;
            if self.selected >= self.scroll_offset + AUTOCOMPLETE_MAX_VISIBLE {
                self.scroll_offset = self.selected + 1 - AUTOCOMPLETE_MAX_VISIBLE;
            }
        }
    }

    /// Get the currently selected match, if any.
    pub fn selected_match(&self) -> Option<&str> {
        self.matches.get(self.selected).map(|s| s.as_str())
    }

    /// The visible window of matches for rendering.
    pub fn visible_matches(&self) -> &[String] {
        let end = (self.scroll_offset + AUTOCOMPLETE_MAX_VISIBLE).min(self.matches.len());
        &self.matches[self.scroll_offset..end]
    }

    /// Refilter matches based on current query using fuzzy substring matching.
    fn refilter(&mut self) {
        let query_lower = self.query.to_lowercase();
        if query_lower.is_empty() {
            // Show all paths (capped for performance)
            self.matches = self.all_paths.iter().take(200).cloned().collect();
        } else {
            // Fuzzy: match if all query chars appear in order in the path
            self.matches = self
                .all_paths
                .iter()
                .filter(|path| fuzzy_match(&query_lower, &path.to_lowercase()))
                .take(200)
                .cloned()
                .collect();
        }
    }
}

/// Simple fuzzy matching: all characters of `query` appear in `haystack` in order.
fn fuzzy_match(query: &str, haystack: &str) -> bool {
    let mut hay_chars = haystack.chars();
    for q_char in query.chars() {
        loop {
            match hay_chars.next() {
                Some(h) if h == q_char => break,
                Some(_) => continue,
                None => return false,
            }
        }
    }
    true
}
