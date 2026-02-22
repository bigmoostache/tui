mod app;
mod infra;
mod llms;
mod modules;
mod state;
mod ui;

use std::io;
use std::sync::mpsc;

use crossterm::{
    ExecutableCommand,
    event::{DisableBracketedPaste, EnableBracketedPaste},
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::prelude::*;

use app::background::TlDrResult;
use app::{App, ensure_default_agent, ensure_default_contexts};
use infra::api::StreamEvent;
use state::cache::CacheUpdate;
use state::persistence::load_state;

fn main() -> io::Result<()> {
    // Parse CLI args
    let args: Vec<String> = std::env::args().collect();
    let resume_stream = args.iter().any(|a| a == "--resume-stream");

    // Handle typst subcommands (used by callback scripts)
    if args.len() >= 2 {
        match args[1].as_str() {
            // Compile a .typ → .pdf in the same directory
            "typst-compile" => return run_typst_compile(&args[2..]),
            // Recompile all .typ files that import a changed template
            "typst-recompile-dependents" => return run_typst_recompile_dependents(&args[2..]),
            _ => {}
        }
    }

    // Panic hook: restore terminal state and log the panic to disk.
    // Without this, a panic leaves the terminal in raw mode + alternate screen,
    // which corrupts the SSH session and the error is lost.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = io::stdout().execute(DisableBracketedPaste);
        let _ = io::stdout().execute(LeaveAlternateScreen);

        // Write panic info to .context-pilot/errors/panic.log
        let error_dir = std::path::Path::new(".context-pilot").join("errors");
        let _ = std::fs::create_dir_all(&error_dir);
        let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let backtrace = std::backtrace::Backtrace::force_capture();
        let msg = format!("[{}] {}\n\n{}\n\n---\n", ts, info, backtrace);
        let log_path = error_dir.join("panic.log");
        let _ = std::fs::OpenOptions::new().create(true).append(true).open(&log_path).and_then(|mut f| {
            use std::io::Write;
            f.write_all(msg.as_bytes())
        });

        default_hook(info);
    }));

    enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;
    io::stdout().execute(EnableBracketedPaste)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;

    let mut state = load_state();

    // Set callback hooks for extracted module crates
    state.highlight_fn = Some(ui::helpers::highlight_file);

    // Validate module dependencies at startup
    modules::validate_dependencies(&state.active_modules);

    // Initialize the ContextType registry from all modules (must happen before any is_fixed/icon/needs_cache calls)
    modules::init_registry();

    // Remove orphaned context elements whose module no longer exists
    // (e.g., tmux panels persisted before the tmux crate was removed).
    {
        let known_types: std::collections::HashSet<String> = modules::all_modules()
            .iter()
            .flat_map(|m| {
                let mut types: Vec<String> =
                    m.dynamic_panel_types().into_iter().map(|ct| ct.as_str().to_string()).collect();
                types.extend(m.fixed_panel_types().into_iter().map(|ct| ct.as_str().to_string()));
                types.extend(m.context_type_metadata().into_iter().map(|meta| meta.context_type.to_string()));
                types
            })
            .collect();
        state.context.retain(|c| known_types.contains(c.context_type.as_str()));
    }

    // Ensure default context elements and seed exist
    ensure_default_contexts(&mut state);
    ensure_default_agent(&mut state);

    // Ensure built-in presets exist on disk
    cp_mod_preset::builtin::ensure_builtin_presets();

    // Create channels
    let (tx, rx) = mpsc::channel::<StreamEvent>();
    let (tldr_tx, tldr_rx) = mpsc::channel::<TlDrResult>();
    let (cache_tx, cache_rx) = mpsc::channel::<CacheUpdate>();

    // Create and run app
    let mut app = App::new(state, cache_tx, resume_stream);
    app.run(&mut terminal, tx, rx, tldr_tx, tldr_rx, cache_rx)?;

    // Cleanup
    disable_raw_mode()?;
    io::stdout().execute(DisableBracketedPaste)?;
    io::stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}

/// Run the typst-compile subcommand: compile a .typ file to PDF in the same directory.
/// Used by the typst-compile callback via $CP_CHANGED_FILES.
/// Usage: cpilot typst-compile <source.typ>
fn run_typst_compile(args: &[String]) -> io::Result<()> {
    if args.is_empty() {
        eprintln!("Usage: cpilot typst-compile <source.typ>");
        std::process::exit(1);
    }

    let source_path = &args[0];

    // Output: same directory, same name, .pdf extension
    let stem = std::path::Path::new(source_path).file_stem().and_then(|s| s.to_str()).unwrap_or("output");
    let parent =
        std::path::Path::new(source_path).parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
    let out = if parent.is_empty() { format!("{}.pdf", stem) } else { format!("{}/{}.pdf", parent, stem) };

    match cp_mod_typst::compiler::compile_and_write(source_path, &out) {
        Ok(msg) => {
            println!("{}", msg);
            Ok(())
        }
        Err(err) => {
            eprint!("{}", err);
            std::process::exit(1);
        }
    }
}

/// Recompile all .typ files in the project that import any of the changed templates.
/// Used by the typst-template-recompile callback via $CP_CHANGED_FILES.
/// Usage: cpilot typst-recompile-dependents <template1.typ> [template2.typ ...]
fn run_typst_recompile_dependents(args: &[String]) -> io::Result<()> {
    if args.is_empty() {
        eprintln!("Usage: cpilot typst-recompile-dependents <template1.typ> [template2.typ ...]");
        std::process::exit(1);
    }

    // Extract just the filename stems from the changed templates
    // e.g., ".context-pilot/shared/typst-templates/report.typ" → "report"
    let changed_stems: Vec<String> = args
        .iter()
        .filter_map(|a| std::path::Path::new(a).file_stem().and_then(|s| s.to_str()).map(|s| s.to_string()))
        .collect();

    if changed_stems.is_empty() {
        println!("No template stems to match.");
        return Ok(());
    }

    // Scan all .typ files in the project, excluding templates dir and target/
    let project_root = std::env::current_dir().unwrap_or_default();
    let mut dependents: Vec<std::path::PathBuf> = Vec::new();
    find_typ_dependents(&project_root, &changed_stems, &mut dependents);

    if dependents.is_empty() {
        println!("No .typ files import changed templates: {:?}", changed_stems);
        return Ok(());
    }

    // Recompile each dependent
    let mut had_error = false;
    for dep in &dependents {
        let source = dep.to_string_lossy().to_string();
        let stem = dep.file_stem().and_then(|s| s.to_str()).unwrap_or("output");
        let parent = dep.parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
        let out = if parent.is_empty() { format!("{}.pdf", stem) } else { format!("{}/{}.pdf", parent, stem) };

        match cp_mod_typst::compiler::compile_and_write(&source, &out) {
            Ok(msg) => println!("{}", msg),
            Err(err) => {
                eprint!("Error compiling {}: {}", source, err);
                had_error = true;
            }
        }
    }

    if had_error {
        std::process::exit(1);
    }
    Ok(())
}

/// Recursively find .typ files that import any of the given template stems.
/// Skips target/, .context-pilot/shared/typst-templates/, and hidden dirs.
fn find_typ_dependents(dir: &std::path::Path, template_stems: &[String], results: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        if path.is_dir() {
            // Skip target, .git, and the templates directory itself
            if name == "target" || name == ".git" || name == "node_modules" {
                continue;
            }
            if path.ends_with(".context-pilot/shared/typst-templates") {
                continue;
            }
            find_typ_dependents(&path, template_stems, results);
        } else if path.extension().and_then(|e| e.to_str()) == Some("typ") {
            // Check if this file imports any of the changed templates
            if let Ok(content) = std::fs::read_to_string(&path)
                && imports_any_template(&content, template_stems)
            {
                results.push(path);
            }
        }
    }
}

/// Check if a .typ file's content imports any of the given template stems.
/// Looks for patterns like:
///   #import ".../<stem>.typ": *
///   #import ".../<stem>.typ": func1, func2
///   #import "@local/templates/<stem>.typ"
fn imports_any_template(content: &str, template_stems: &[String]) -> bool {
    for line in content.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("#import") {
            continue;
        }
        for stem in template_stems {
            // Match imports that reference the template filename
            let pattern = format!("{}.typ", stem);
            if trimmed.contains(&pattern) {
                return true;
            }
        }
    }
    false
}
