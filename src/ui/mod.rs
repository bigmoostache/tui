pub use crate::infra::constants::chars;
pub mod help;
pub mod helpers;
mod input;
pub mod markdown;
pub mod perf;
mod sidebar;
pub use crate::infra::constants::theme;
pub mod typewriter;

use ratatui::{
    prelude::*,
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};

use crate::app::panels;
use crate::infra::constants::{SIDEBAR_WIDTH, STATUS_BAR_HEIGHT};
use crate::state::{ContextType, State};
use crate::ui::perf::PERF;

pub fn render(frame: &mut Frame, state: &mut State) {
    PERF.frame_start();
    let _guard = crate::profile!("ui::render");
    let area = frame.area();

    // Fill base background
    frame.render_widget(Block::default().style(Style::default().bg(theme::bg_base())), area);

    // Main layout: body + footer (no header)
    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),                    // Body
            Constraint::Length(STATUS_BAR_HEIGHT), // Status bar
        ])
        .split(area);

    render_body(frame, state, main_layout[0]);
    input::render_status_bar(frame, state, main_layout[1]);

    // Render performance overlay if enabled
    if state.perf_enabled {
        perf::render_perf_overlay(frame, area);
    }

    // Render config overlay if open
    if state.config_view {
        render_config_overlay(frame, state, area);
    }

    PERF.frame_end();
}

fn render_body(frame: &mut Frame, state: &mut State, area: Rect) {
    // Body layout: sidebar + main content
    let body_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(SIDEBAR_WIDTH), // Sidebar
            Constraint::Min(1),                // Main content
        ])
        .split(area);

    sidebar::render_sidebar(frame, state, body_layout[0]);
    render_main_content(frame, state, body_layout[1]);
}

fn render_main_content(frame: &mut Frame, state: &mut State, area: Rect) {
    // Check if question form is active — render it at bottom of content area
    if let Some(form) = state.get_ext::<cp_base::ui::PendingQuestionForm>()
        && !form.resolved
    {
        // Split: content panel on top, question form at bottom
        let form_height = input::calculate_question_form_height(form);
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(3),              // Content panel (shrinks)
                Constraint::Length(form_height), // Question form
            ])
            .split(area);

        render_content_panel(frame, state, layout[0]);
        // Indent form by 1 col to avoid overlapping sidebar border
        let form_area = Rect { x: layout[1].x + 1, width: layout[1].width.saturating_sub(1), ..layout[1] };
        input::render_question_form(frame, state, form_area);
        return;
    }

    // Normal rendering — no separate input box, panels handle their own
    render_content_panel(frame, state, area);
}

fn render_content_panel(frame: &mut Frame, state: &mut State, area: Rect) {
    let _guard = crate::profile!("ui::render_panel");
    let context_type = state
        .context
        .get(state.selected_context)
        .map(|c| c.context_type.clone())
        .unwrap_or(ContextType::new(ContextType::CONVERSATION));

    let panel = panels::get_panel(&context_type);

    // ConversationPanel overrides render() with custom scrollbar + caching.
    // All other panels use render_panel_default (which calls panel.content()).
    if context_type == ContextType::CONVERSATION {
        panel.render(frame, state, area);
    } else {
        panels::render_panel_default(panel.as_ref(), frame, state, area);
    }
}

fn render_config_overlay(frame: &mut Frame, state: &State, area: Rect) {
    use crate::llms::{AnthropicModel, DeepSeekModel, GrokModel, GroqModel, LlmProvider};

    // Center the overlay, clamped to available area
    let overlay_width = 56u16.min(area.width);
    let overlay_height = 45u16.min(area.height);
    let x = area.x + area.width.saturating_sub(overlay_width) / 2;
    let y = area.y + area.height.saturating_sub(overlay_height) / 2;
    let overlay_area = Rect::new(x, y, overlay_width, overlay_height);

    let mut lines: Vec<Line> = Vec::new();

    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled("  LLM Provider", Style::default().fg(theme::text_secondary()).bold())]));
    lines.push(Line::from(""));

    // Provider options
    let providers = [
        (LlmProvider::Anthropic, "1", "Anthropic Claude"),
        (LlmProvider::ClaudeCode, "2", "Claude Code (OAuth)"),
        (LlmProvider::ClaudeCodeApiKey, "6", "Claude Code (API Key)"),
        (LlmProvider::Grok, "3", "Grok (xAI)"),
        (LlmProvider::Groq, "4", "Groq"),
        (LlmProvider::DeepSeek, "5", "DeepSeek"),
    ];

    for (provider, key, name) in providers {
        let is_selected = state.llm_provider == provider;
        let indicator = if is_selected { ">" } else { " " };
        let check = if is_selected { "[x]" } else { "[ ]" };

        let style =
            if is_selected { Style::default().fg(theme::accent()).bold() } else { Style::default().fg(theme::text()) };

        lines.push(Line::from(vec![
            Span::styled(format!("  {} ", indicator), Style::default().fg(theme::accent())),
            Span::styled(format!("{} ", key), Style::default().fg(theme::warning())),
            Span::styled(format!("{} ", check), style),
            Span::styled(name.to_string(), style),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        format!("  {}", chars::HORIZONTAL.repeat(50)),
        Style::default().fg(theme::border()),
    )]));
    lines.push(Line::from(""));

    // Model selection based on current provider
    lines.push(Line::from(vec![Span::styled("  Model", Style::default().fg(theme::text_secondary()).bold())]));
    lines.push(Line::from(""));

    match state.llm_provider {
        LlmProvider::Anthropic | LlmProvider::ClaudeCode | LlmProvider::ClaudeCodeApiKey => {
            for (model, key) in [
                (AnthropicModel::ClaudeOpus45, "a"),
                (AnthropicModel::ClaudeSonnet45, "b"),
                (AnthropicModel::ClaudeHaiku45, "c"),
            ] {
                let is_selected = state.anthropic_model == model;
                render_model_line_with_info(&mut lines, is_selected, key, &model);
            }
        }
        LlmProvider::Grok => {
            for (model, key) in [(GrokModel::Grok41Fast, "a"), (GrokModel::Grok4Fast, "b")] {
                let is_selected = state.grok_model == model;
                render_model_line_with_info(&mut lines, is_selected, key, &model);
            }
        }
        LlmProvider::Groq => {
            for (model, key) in [
                (GroqModel::GptOss120b, "a"),
                (GroqModel::GptOss20b, "b"),
                (GroqModel::Llama33_70b, "c"),
                (GroqModel::Llama31_8b, "d"),
            ] {
                let is_selected = state.groq_model == model;
                render_model_line_with_info(&mut lines, is_selected, key, &model);
            }
        }
        LlmProvider::DeepSeek => {
            for (model, key) in [(DeepSeekModel::DeepseekChat, "a"), (DeepSeekModel::DeepseekReasoner, "b")] {
                let is_selected = state.deepseek_model == model;
                render_model_line_with_info(&mut lines, is_selected, key, &model);
            }
        }
    }

    // API check status
    lines.push(Line::from(""));
    if state.api_check_in_progress {
        let spinner_chars = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
        let spinner = spinner_chars[(state.spinner_frame as usize) % spinner_chars.len()];
        lines.push(Line::from(vec![
            Span::styled(format!("  {} ", spinner), Style::default().fg(theme::accent())),
            Span::styled("Checking API...", Style::default().fg(theme::text_muted())),
        ]));
    } else if let Some(result) = &state.api_check_result {
        use crate::infra::config::normalize_icon;
        let (icon, color, msg) = if result.all_ok() {
            (normalize_icon("✓"), theme::success(), "API OK")
        } else if let Some(err) = &result.error {
            (normalize_icon("✗"), theme::error(), err.as_str())
        } else {
            let mut issues = Vec::new();
            if !result.auth_ok {
                issues.push("auth");
            }
            if !result.streaming_ok {
                issues.push("streaming");
            }
            if !result.tools_ok {
                issues.push("tools");
            }
            (normalize_icon("!"), theme::warning(), if issues.is_empty() { "Unknown issue" } else { "Issues detected" })
        };
        lines.push(Line::from(vec![
            Span::styled(format!("  {}", icon), Style::default().fg(color)),
            Span::styled(msg.to_string(), Style::default().fg(color)),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        format!("  {}", chars::HORIZONTAL.repeat(50)),
        Style::default().fg(theme::border()),
    )]));
    lines.push(Line::from(""));

    // Helper to format token count
    let format_tokens = |tokens: usize| -> String {
        if tokens >= 1_000_000 {
            format!("{:.1}M", tokens as f64 / 1_000_000.0)
        } else if tokens >= 1_000 {
            format!("{}K", tokens / 1_000)
        } else {
            format!("{}", tokens)
        }
    };

    let bar_width = 24usize;
    let max_budget = state.model_context_window();
    let effective_budget = state.effective_context_budget();
    let selected = state.config_selected_bar;

    // Helper to render a progress bar with selection indicator
    let render_bar = |lines: &mut Vec<Line>,
                      idx: usize,
                      label: &str,
                      pct: usize,
                      filled: usize,
                      tokens: usize,
                      bar_color: Color,
                      extra: Option<&str>| {
        let is_selected = selected == idx;
        let indicator = if is_selected { ">" } else { " " };
        let label_style = if is_selected {
            Style::default().fg(theme::accent()).bold()
        } else {
            Style::default().fg(theme::text_secondary()).bold()
        };
        let arrow_color = if is_selected { theme::accent() } else { theme::text_muted() };

        lines.push(Line::from(vec![
            Span::styled(format!(" {} ", indicator), Style::default().fg(theme::accent())),
            Span::styled(label.to_string(), label_style),
        ]));
        lines.push(Line::from(vec![
            Span::styled("   ◀ ", Style::default().fg(arrow_color)),
            Span::styled(chars::BLOCK_FULL.repeat(filled.min(bar_width)), Style::default().fg(bar_color)),
            Span::styled(
                chars::BLOCK_LIGHT.repeat(bar_width.saturating_sub(filled)),
                Style::default().fg(theme::bg_elevated()),
            ),
            Span::styled(" ▶ ", Style::default().fg(arrow_color)),
            Span::styled(format!("{}%", pct), Style::default().fg(theme::text()).bold()),
            Span::styled(
                format!("  {} tok{}", format_tokens(tokens), extra.unwrap_or("")),
                Style::default().fg(theme::text_muted()),
            ),
        ]));
    };

    // 1. Context Budget
    let budget_pct = (effective_budget as f64 / max_budget as f64 * 100.0) as usize;
    let budget_filled = ((effective_budget as f64 / max_budget as f64) * bar_width as f64) as usize;
    render_bar(&mut lines, 0, "Context Budget", budget_pct, budget_filled, effective_budget, theme::success(), None);

    // 2. Cleaning Threshold
    let threshold_pct = (state.cleaning_threshold * 100.0) as usize;
    let threshold_tokens = state.cleaning_threshold_tokens();
    let threshold_filled = ((state.cleaning_threshold * bar_width as f32) as usize).min(bar_width);
    render_bar(
        &mut lines,
        1,
        "Clean Trigger",
        threshold_pct,
        threshold_filled,
        threshold_tokens,
        theme::warning(),
        None,
    );

    // 3. Target Cleaning
    let target_pct = (state.cleaning_target_proportion * 100.0) as usize;
    let target_tokens = state.cleaning_target_tokens();
    let target_abs_pct = (state.cleaning_target() * 100.0) as usize;
    let target_filled = ((state.cleaning_target_proportion * bar_width as f32) as usize).min(bar_width);
    let extra = format!(" ({}%)", target_abs_pct);
    render_bar(&mut lines, 2, "Clean Target", target_pct, target_filled, target_tokens, theme::accent(), Some(&extra));

    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        format!("  {}", chars::HORIZONTAL.repeat(50)),
        Style::default().fg(theme::border()),
    )]));
    lines.push(Line::from(""));

    // Theme selection
    lines.push(Line::from(vec![Span::styled("  Theme", Style::default().fg(theme::text_secondary()).bold())]));
    lines.push(Line::from(""));

    // Show current theme with preview icons
    {
        use crate::infra::config::{THEME_ORDER, get_theme};
        let current_theme = get_theme(&state.active_theme);
        let fallback_icon = "📄".to_string();

        // Show theme name and preview icons
        lines.push(Line::from(vec![
            Span::styled("   ◀ ", Style::default().fg(theme::accent())),
            Span::styled(format!("{:<12}", current_theme.name), Style::default().fg(theme::accent()).bold()),
            Span::styled(" ▶  ", Style::default().fg(theme::accent())),
            Span::styled(
                format!(
                    "{} {} {} {}",
                    current_theme.messages.user,
                    current_theme.messages.assistant,
                    current_theme.context.get("tree").unwrap_or(&fallback_icon),
                    current_theme.context.get("file").unwrap_or(&fallback_icon),
                ),
                Style::default().fg(theme::text()),
            ),
        ]));
        lines.push(Line::from(vec![Span::styled(
            format!("     {}", current_theme.description),
            Style::default().fg(theme::text_muted()),
        )]));

        // Show position in theme list
        let current_idx = THEME_ORDER.iter().position(|&t| t == state.active_theme).unwrap_or(0);
        lines.push(Line::from(vec![Span::styled(
            format!("     ({}/{})", current_idx + 1, THEME_ORDER.len()),
            Style::default().fg(theme::text_muted()),
        )]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        format!("  {}", chars::HORIZONTAL.repeat(50)),
        Style::default().fg(theme::border()),
    )]));

    // Help text
    lines.push(Line::from(vec![
        Span::styled("  ", Style::default()),
        Span::styled("1-5", Style::default().fg(theme::warning())),
        Span::styled(" provider  ", Style::default().fg(theme::text_muted())),
        Span::styled("a-d", Style::default().fg(theme::warning())),
        Span::styled(" model  ", Style::default().fg(theme::text_muted())),
        Span::styled("t", Style::default().fg(theme::warning())),
        Span::styled(" theme", Style::default().fg(theme::text_muted())),
    ]));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::accent()))
        .style(Style::default().bg(theme::bg_surface()))
        .title(Span::styled(" Configuration ", Style::default().fg(theme::accent()).bold()));

    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(Clear, overlay_area);
    frame.render_widget(paragraph, overlay_area);
}

fn render_model_line_with_info<M: crate::llms::ModelInfo>(
    lines: &mut Vec<Line>,
    is_selected: bool,
    key: &str,
    model: &M,
) {
    let indicator = if is_selected { ">" } else { " " };
    let check = if is_selected { "[x]" } else { "[ ]" };

    let style =
        if is_selected { Style::default().fg(theme::accent()).bold() } else { Style::default().fg(theme::text()) };

    // Format context window (e.g., "200K" or "2M")
    let ctx = model.context_window();
    let ctx_str = if ctx >= 1_000_000 { format!("{}M", ctx / 1_000_000) } else { format!("{}K", ctx / 1_000) };

    // Format pricing info
    let price_str = format!("${:.0}/${:.0}", model.input_price_per_mtok(), model.output_price_per_mtok());

    lines.push(Line::from(vec![
        Span::styled(format!("  {} ", indicator), Style::default().fg(theme::accent())),
        Span::styled(format!("{} ", key), Style::default().fg(theme::warning())),
        Span::styled(format!("{} ", check), style),
        Span::styled(format!("{:<12}", model.display_name()), style),
        Span::styled(format!("{:>4} ", ctx_str), Style::default().fg(theme::text_muted())),
        Span::styled(price_str, Style::default().fg(theme::text_muted())),
    ]));
}
