/// Panel data persistence module
/// Handles loading and saving panel files (panels/{uid}.json)
/// Includes conversation panels and dynamic panels (File, Glob, Grep, Tmux)

use std::fs;
use std::path::PathBuf;

use crate::constants::{STORE_DIR, PANELS_DIR};
use crate::state::PanelData;

fn panels_dir() -> PathBuf {
    PathBuf::from(STORE_DIR).join(PANELS_DIR)
}

fn panel_path(uid: &str) -> PathBuf {
    panels_dir().join(format!("{}.json", uid))
}

/// Load panel data by UID from panels/{uid}.json
pub fn load_panel(uid: &str) -> Option<PanelData> {
    let path = panel_path(uid);
    let json = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&json).ok()
}

/// Save panel data to panels/{uid}.json
pub fn save_panel(panel: &PanelData) {
    let dir = panels_dir();
    fs::create_dir_all(&dir).ok();
    let path = panel_path(&panel.uid);
    if let Ok(json) = serde_json::to_string_pretty(panel) {
        fs::write(path, json).ok();
    }
}

/// Delete panel files in panels/ that are not in `known_uids`.
pub fn delete_orphan_panels(known_uids: &std::collections::HashSet<String>) {
    let dir = panels_dir();
    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            if !known_uids.contains(stem) {
                let _ = fs::remove_file(&path);
            }
        }
    }
}
