use ratatui::prelude::*;

use crate::modules::all_modules;
use crate::state::{State, get_context_type_meta};
use crate::ui::{
    chars,
    helpers::{Cell, format_number, render_table},
    theme,
};

/// Horizontal separator line.
pub fn separator() -> Vec<Line<'static>> {
    vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            format!(" {}", chars::HORIZONTAL.repeat(60)),
            Style::default().fg(theme::border()),
        )]),
        Line::from(""),
    ]
}

/// Render the TOKEN USAGE section with progress bar.
pub fn render_token_usage(state: &State, base_style: Style) -> Vec<Line<'static>> {
    let mut text: Vec<Line> = Vec::new();

    let total_tokens: usize = state.context.iter().map(|c| c.token_count).sum();
    let budget = state.effective_context_budget();
    let threshold = state.cleaning_threshold_tokens();
    let usage_pct = (total_tokens as f64 / budget as f64 * 100.0).min(100.0);

    text.push(Line::from(vec![
        Span::styled(" ".to_string(), base_style),
        Span::styled("TOKEN USAGE".to_string(), Style::default().fg(theme::text_muted()).bold()),
    ]));
    text.push(Line::from(""));

    let current = format_number(total_tokens);
    let threshold_str = format_number(threshold);
    let budget_str = format_number(budget);
    let pct = format!("{:.1}%", usage_pct);

    text.push(Line::from(vec![
        Span::styled(" ".to_string(), base_style),
        Span::styled(current, Style::default().fg(theme::text()).bold()),
        Span::styled(" / ".to_string(), Style::default().fg(theme::text_muted())),
        Span::styled(threshold_str, Style::default().fg(theme::warning())),
        Span::styled(" / ".to_string(), Style::default().fg(theme::text_muted())),
        Span::styled(budget_str, Style::default().fg(theme::accent()).bold()),
        Span::styled(format!(" ({})", pct), Style::default().fg(theme::text_muted())),
    ]));

    // Progress bar with threshold marker
    let bar_width = 60usize;
    let threshold_pct = state.cleaning_threshold;
    let filled = ((usage_pct / 100.0) * bar_width as f64) as usize;
    let threshold_pos = (threshold_pct as f64 * bar_width as f64) as usize;

    let bar_color = if total_tokens >= threshold {
        theme::error()
    } else if total_tokens as f64 >= threshold as f64 * 0.9 {
        theme::warning()
    } else {
        theme::accent()
    };

    let mut bar_spans = vec![Span::styled(" ".to_string(), base_style)];
    for i in 0..bar_width {
        let char = if i == threshold_pos && threshold_pos < bar_width {
            "|" // Threshold marker
        } else if i < filled {
            chars::BLOCK_FULL
        } else {
            chars::BLOCK_LIGHT
        };

        let color = if i == threshold_pos {
            theme::warning()
        } else if i < filled {
            bar_color
        } else {
            theme::bg_elevated()
        };

        bar_spans.push(Span::styled(char, Style::default().fg(color)));
    }
    text.push(Line::from(bar_spans));

    text
}

/// Render the GIT STATUS section (branch only).
pub fn render_git_status(state: &State, base_style: Style) -> Vec<Line<'static>> {
    let mut text: Vec<Line> = Vec::new();
    let gs = cp_mod_git::GitState::get(state);

    if !gs.git_is_repo {
        return text;
    }

    text.push(Line::from(vec![
        Span::styled(" ".to_string(), base_style),
        Span::styled("GIT".to_string(), Style::default().fg(theme::text_muted()).bold()),
    ]));
    text.push(Line::from(""));

    // Branch name
    if let Some(branch) = &gs.git_branch {
        let branch_color = if branch.starts_with("detached:") { theme::warning() } else { theme::accent() };
        text.push(Line::from(vec![
            Span::styled(" ".to_string(), base_style),
            Span::styled("Branch: ".to_string(), Style::default().fg(theme::text_secondary())),
            Span::styled(branch.clone(), Style::default().fg(branch_color).bold()),
        ]));
    }

    text
}

/// Render the CONTEXT ELEMENTS section.
pub fn render_context_elements(state: &State, base_style: Style) -> Vec<Line<'static>> {
    let mut text: Vec<Line> = Vec::new();

    text.push(Line::from(vec![
        Span::styled(" ".to_string(), base_style),
        Span::styled("CONTEXT ELEMENTS".to_string(), Style::default().fg(theme::text_muted()).bold()),
    ]));
    text.push(Line::from(""));

    let header = [
        Cell::new("ID", Style::default()),
        Cell::new("Type", Style::default()),
        Cell::right("Tokens", Style::default()),
        Cell::right("Cost", Style::default()),
        Cell::new("Hit", Style::default()),
        Cell::new("Refreshed", Style::default()),
        Cell::new("Details", Style::default()),
    ];

    // Sort by last_refresh_ms ascending (oldest first = same order LLM sees them)
    let mut sorted_contexts: Vec<&crate::state::ContextElement> = state.context.iter().collect();
    sorted_contexts.sort_by_key(|ctx| ctx.last_refresh_ms);

    let now_ms = crate::app::panels::now_ms();

    let modules = all_modules();

    let rows: Vec<Vec<Cell>> = sorted_contexts
        .iter()
        .map(|ctx| {
            // Look up display_name from registry, fallback to raw context type string
            let type_name = get_context_type_meta(ctx.context_type.as_str())
                .map(|m| m.display_name)
                .unwrap_or(ctx.context_type.as_str());

            // Ask modules for detail string
            let details = modules.iter().find_map(|m| m.context_detail(ctx)).unwrap_or_default();

            let truncated_details = if details.len() > 30 {
                format!("{}...", &details[..details.floor_char_boundary(27)])
            } else {
                details
            };

            // Format refresh time as relative
            let refreshed = if ctx.last_refresh_ms < 1577836800000 {
                "—".to_string()
            } else if now_ms > ctx.last_refresh_ms {
                crate::ui::helpers::format_time_ago(now_ms - ctx.last_refresh_ms)
            } else {
                "now".to_string()
            };

            let icon = ctx.context_type.icon();
            let id_with_icon = format!("{}{}", icon, ctx.id);

            let cost_str = format!("${:.2}", ctx.panel_total_cost);
            let (hit_str, hit_color) =
                if ctx.panel_cache_hit { ("\u{2713}", theme::success()) } else { ("\u{2717}", theme::error()) };

            vec![
                Cell::new(id_with_icon, Style::default().fg(theme::accent_dim())),
                Cell::new(type_name, Style::default().fg(theme::text_secondary())),
                Cell::right(format_number(ctx.token_count), Style::default().fg(theme::accent())),
                Cell::right(cost_str, Style::default().fg(theme::text_muted())),
                Cell::new(hit_str, Style::default().fg(hit_color)),
                Cell::new(refreshed, Style::default().fg(theme::text_muted())),
                Cell::new(truncated_details, Style::default().fg(theme::text_muted())),
            ]
        })
        .collect();

    text.extend(render_table(&header, &rows, None, 1));

    text
}

pub use super::render_details::render_statistics;
