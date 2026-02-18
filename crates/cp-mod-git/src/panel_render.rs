use std::collections::HashMap;

use ratatui::prelude::*;

use cp_base::config::{chars, theme};
use cp_base::state::State;

use crate::types::{GitChangeType, GitState};

/// Format git status for LLM context (from raw change tuples — used in cache refresh)
pub(crate) fn format_git_content_for_cache(
    branch: &Option<String>,
    changes: &[(String, i32, i32, GitChangeType, String)],
    show_diffs: bool,
) -> String {
    let mut output = String::new();
    if let Some(branch) = branch {
        output.push_str(&format!("Branch: {}\n", branch));
    }
    if changes.is_empty() {
        output.push_str("\nWorking tree clean\n");
    } else {
        output.push_str("\n| File | Type | + | - | Net |\n");
        output.push_str("|------|------|---|---|-----|\n");
        let mut total_add: i32 = 0;
        let mut total_del: i32 = 0;
        for (path, additions, deletions, change_type, _) in changes {
            total_add += additions;
            total_del += deletions;
            let net = additions - deletions;
            let net_str = if net >= 0 { format!("+{}", net) } else { format!("{}", net) };
            let type_str = match change_type {
                GitChangeType::Added => "A",
                GitChangeType::Untracked => "U",
                GitChangeType::Deleted => "D",
                GitChangeType::Modified => "M",
                GitChangeType::Renamed => "R",
            };
            output.push_str(&format!("| {} | {} | +{} | -{} | {} |\n", path, type_str, additions, deletions, net_str));
        }
        let total_net = total_add - total_del;
        let total_net_str = if total_net >= 0 { format!("+{}", total_net) } else { format!("{}", total_net) };
        output
            .push_str(&format!("| **Total** | | **+{}** | **-{}** | **{}** |\n", total_add, total_del, total_net_str));
        if show_diffs {
            output.push_str("\n## Diffs\n\n");
            for (_, _, _, _, diff_content) in changes {
                if !diff_content.is_empty() {
                    output.push_str("```diff\n");
                    output.push_str(diff_content);
                    output.push_str("```\n\n");
                }
            }
        }
    }
    output
}

/// Parse unified diff output and group by file
pub(crate) fn parse_diff_by_file(diff_output: &str, diff_contents: &mut HashMap<String, String>) {
    let mut current_file: Option<String> = None;
    let mut current_diff = String::new();
    for line in diff_output.lines() {
        if line.starts_with("diff --git") {
            if let Some(file) = current_file.take()
                && !current_diff.is_empty()
            {
                diff_contents.insert(file, current_diff.clone());
            }
            current_diff.clear();
            if let Some(b_part) = line.split(" b/").nth(1) {
                current_file = Some(b_part.to_string());
            }
            current_diff.push_str(line);
            current_diff.push('\n');
        } else if current_file.is_some() {
            current_diff.push_str(line);
            current_diff.push('\n');
        }
    }
    if let Some(file) = current_file
        && !current_diff.is_empty()
    {
        diff_contents.insert(file, current_diff);
    }
}

/// Parse git diff --numstat output and add to file_changes map
pub(crate) fn parse_numstat_to_map(output: &str, file_changes: &mut HashMap<String, (i32, i32, GitChangeType)>) {
    for line in output.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 3 {
            let add: i32 = parts[0].parse().unwrap_or(0);
            let del: i32 = parts[1].parse().unwrap_or(0);
            let path = parts[2].to_string();
            let path = if path.contains(" => ") {
                path.split(" => ").last().unwrap_or(&path).trim_end_matches('}').to_string()
            } else {
                path
            };
            let entry = file_changes.entry(path).or_insert((0, 0, GitChangeType::Modified));
            entry.0 += add;
            entry.1 += del;
        }
    }
}

/// Render the Git panel content for the TUI display.
pub(crate) fn render_git_panel_content(state: &State, base_style: Style) -> Vec<Line<'static>> {
    let gs = GitState::get(state);
    let mut text: Vec<Line> = Vec::new();

    if !gs.git_is_repo {
        text.push(Line::from(vec![
            Span::styled(" ".to_string(), base_style),
            Span::styled("Not a git repository".to_string(), Style::default().fg(theme::text_muted()).italic()),
        ]));
        return text;
    }

    // Branch name
    if let Some(branch) = &gs.git_branch {
        let branch_color = if branch.starts_with("detached:") { theme::warning() } else { theme::accent() };
        text.push(Line::from(vec![
            Span::styled(" ".to_string(), base_style),
            Span::styled("Branch: ".to_string(), Style::default().fg(theme::text_secondary())),
            Span::styled(branch.clone(), Style::default().fg(branch_color).bold()),
        ]));
    }

    // All branches
    if !gs.git_branches.is_empty() {
        text.push(Line::from(""));
        text.push(Line::from(vec![
            Span::styled(" ".to_string(), base_style),
            Span::styled("Branches:".to_string(), Style::default().fg(theme::text_secondary()).bold()),
        ]));
        for (branch_name, is_current) in &gs.git_branches {
            let (prefix, style) = if *is_current {
                ("* ", Style::default().fg(theme::accent()).bold())
            } else {
                ("  ", Style::default().fg(theme::text_muted()))
            };
            text.push(Line::from(vec![
                Span::styled(" ".to_string(), base_style),
                Span::styled(prefix.to_string(), style),
                Span::styled(branch_name.clone(), style),
            ]));
        }
    }

    text.push(Line::from(""));

    if gs.git_file_changes.is_empty() {
        text.push(Line::from(vec![
            Span::styled(" ".to_string(), base_style),
            Span::styled("Working tree clean".to_string(), Style::default().fg(theme::success())),
        ]));
        return text;
    }

    // Calculate column widths
    let path_width = gs.git_file_changes.iter().map(|f| f.path.len()).max().unwrap_or(4).clamp(4, 45);

    // Table header
    text.push(Line::from(vec![
        Span::styled(" ".to_string(), base_style),
        Span::styled("T ".to_string(), Style::default().fg(theme::text_secondary()).bold()),
        Span::styled(
            format!("{:<width$}", "File", width = path_width),
            Style::default().fg(theme::text_secondary()).bold(),
        ),
        Span::styled("  ", base_style),
        Span::styled(format!("{:>6}", "+"), Style::default().fg(theme::success()).bold()),
        Span::styled("  ", base_style),
        Span::styled(format!("{:>6}", "-"), Style::default().fg(theme::error()).bold()),
        Span::styled("  ", base_style),
        Span::styled(format!("{:>6}", "Net"), Style::default().fg(theme::text_secondary()).bold()),
    ]));

    // Separator
    text.push(Line::from(vec![
        Span::styled(" ".to_string(), base_style),
        Span::styled(chars::HORIZONTAL.repeat(path_width + 30), Style::default().fg(theme::border())),
    ]));

    // File rows
    let mut total_add: i32 = 0;
    let mut total_del: i32 = 0;

    for file in &gs.git_file_changes {
        total_add += file.additions;
        total_del += file.deletions;
        let net = file.additions - file.deletions;

        let (type_char, type_color) = match file.change_type {
            GitChangeType::Added => ("A", theme::success()),
            GitChangeType::Untracked => ("U", theme::success()),
            GitChangeType::Deleted => ("D", theme::error()),
            GitChangeType::Modified => ("M", theme::warning()),
            GitChangeType::Renamed => ("R", theme::accent()),
        };

        let display_path = if file.path.len() > path_width {
            format!("...{}", &file.path[file.path.len() - path_width + 3..])
        } else {
            file.path.clone()
        };

        let net_color = if net > 0 {
            theme::success()
        } else if net < 0 {
            theme::error()
        } else {
            theme::text_muted()
        };

        let net_str = if net > 0 { format!("+{}", net) } else { format!("{}", net) };

        text.push(Line::from(vec![
            Span::styled(" ".to_string(), base_style),
            Span::styled(format!("{} ", type_char), Style::default().fg(type_color)),
            Span::styled(
                format!("{:<width$}", display_path, width = path_width),
                Style::default().fg(theme::text()),
            ),
            Span::styled("  ", base_style),
            Span::styled(format!("{:>6}", format!("+{}", file.additions)), Style::default().fg(theme::success())),
            Span::styled("  ", base_style),
            Span::styled(format!("{:>6}", format!("-{}", file.deletions)), Style::default().fg(theme::error())),
            Span::styled("  ", base_style),
            Span::styled(format!("{:>6}", net_str), Style::default().fg(net_color)),
        ]));
    }

    // Total row separator
    text.push(Line::from(vec![
        Span::styled(" ".to_string(), base_style),
        Span::styled(chars::HORIZONTAL.repeat(path_width + 30), Style::default().fg(theme::border())),
    ]));

    // Total row
    let total_net = total_add - total_del;
    let total_net_color = if total_net > 0 {
        theme::success()
    } else if total_net < 0 {
        theme::error()
    } else {
        theme::text_muted()
    };
    let total_net_str = if total_net > 0 { format!("+{}", total_net) } else { format!("{}", total_net) };

    text.push(Line::from(vec![
        Span::styled(" ".to_string(), base_style),
        Span::styled("  ".to_string(), base_style),
        Span::styled(format!("{:<width$}", "Total", width = path_width), Style::default().fg(theme::text()).bold()),
        Span::styled("  ", base_style),
        Span::styled(format!("{:>6}", format!("+{}", total_add)), Style::default().fg(theme::success()).bold()),
        Span::styled("  ", base_style),
        Span::styled(format!("{:>6}", format!("-{}", total_del)), Style::default().fg(theme::error()).bold()),
        Span::styled("  ", base_style),
        Span::styled(format!("{:>6}", total_net_str), Style::default().fg(total_net_color).bold()),
    ]));

    // Summary stats
    text.push(Line::from(""));
    let added = gs.git_file_changes.iter().filter(|f| f.change_type == GitChangeType::Added).count();
    let untracked = gs.git_file_changes.iter().filter(|f| f.change_type == GitChangeType::Untracked).count();
    let modified = gs.git_file_changes.iter().filter(|f| f.change_type == GitChangeType::Modified).count();
    let deleted = gs.git_file_changes.iter().filter(|f| f.change_type == GitChangeType::Deleted).count();
    let renamed = gs.git_file_changes.iter().filter(|f| f.change_type == GitChangeType::Renamed).count();

    let mut summary_parts = Vec::new();
    if added > 0 {
        summary_parts.push(format!("{} added", added));
    }
    if untracked > 0 {
        summary_parts.push(format!("{} untracked", untracked));
    }
    if modified > 0 {
        summary_parts.push(format!("{} modified", modified));
    }
    if deleted > 0 {
        summary_parts.push(format!("{} deleted", deleted));
    }
    if renamed > 0 {
        summary_parts.push(format!("{} renamed", renamed));
    }

    if !summary_parts.is_empty() {
        text.push(Line::from(vec![
            Span::styled(" ".to_string(), base_style),
            Span::styled(summary_parts.join(", "), Style::default().fg(theme::text_muted())),
        ]));
    }

    // Git log (if enabled)
    if gs.git_show_logs {
        text.push(Line::from(""));
        text.push(Line::from(vec![
            Span::styled(" ".to_string(), base_style),
            Span::styled(chars::HORIZONTAL.repeat(60), Style::default().fg(theme::border())),
        ]));
        text.push(Line::from(vec![
            Span::styled(" ".to_string(), base_style),
            Span::styled("Recent Commits:".to_string(), Style::default().fg(theme::text_secondary()).bold()),
        ]));

        if let Some(log_content) = &gs.git_log_content {
            for line in log_content.lines() {
                text.push(Line::from(vec![
                    Span::styled(" ".to_string(), base_style),
                    Span::styled(line.to_string(), Style::default().fg(theme::text_muted())),
                ]));
            }
        }
    }

    // Display diff content for each file
    for file in &gs.git_file_changes {
        if file.diff_content.is_empty() {
            continue;
        }

        text.push(Line::from(""));
        text.push(Line::from(vec![
            Span::styled(" ".to_string(), base_style),
            Span::styled(chars::HORIZONTAL.repeat(60), Style::default().fg(theme::border())),
        ]));

        // Render diff with syntax highlighting
        for line in file.diff_content.lines() {
            let (style, display_line) = if line.starts_with("+++") || line.starts_with("---") {
                (Style::default().fg(theme::text_secondary()).bold(), line.to_string())
            } else if line.starts_with("@@") {
                (Style::default().fg(theme::accent()), line.to_string())
            } else if line.starts_with('+') && !line.starts_with("+++") {
                (Style::default().fg(theme::success()), line.to_string())
            } else if line.starts_with('-') && !line.starts_with("---") {
                (Style::default().fg(theme::error()), line.to_string())
            } else if line.starts_with("diff --git") {
                (Style::default().fg(theme::accent()).bold(), line.to_string())
            } else if line.starts_with("new file") || line.starts_with("deleted file") || line.starts_with("index ")
            {
                (Style::default().fg(theme::text_muted()), line.to_string())
            } else {
                (Style::default().fg(theme::text_muted()), line.to_string())
            };

            text.push(Line::from(vec![
                Span::styled(" ".to_string(), base_style),
                Span::styled(display_line, style),
            ]));
        }
    }

    text
}
