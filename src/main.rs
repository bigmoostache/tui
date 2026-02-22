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

    // Handle typst-compile subcommand (used by callback script)
    if args.len() >= 2 && args[1] == "typst-compile" {
        return run_typst_compile(&args[2..]);
    }

    // Handle typst-compile-template subcommand (recompile all docs using a template)
    if args.len() >= 2 && args[1] == "typst-compile-template" {
        return run_typst_compile_template(&args[2..]);
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

/// Run the typst-compile subcommand: compile a .typ file to PDF.
/// Used by the typst-compile callback.
/// Usage: cpilot typst-compile <source.typ> [--output <out.pdf>] [--target <target.pdf>]
///
/// If --target is not provided, looks up the target in config.json under modules.typst.documents.
fn run_typst_compile(args: &[String]) -> io::Result<()> {
    if args.is_empty() {
        eprintln!("Usage: cpilot typst-compile <source.typ> [--output <out.pdf>]");
        std::process::exit(1);
    }

    let source_path = &args[0];
    let mut output_path: Option<String> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--output" if i + 1 < args.len() => {
                output_path = Some(args[i + 1].clone());
                i += 2;
            }
            _ => {
                eprintln!("Unknown argument: {}", args[i]);
                std::process::exit(1);
            }
        }
    }

    // Determine output: explicit --output > target from config.json > default
    let out = output_path.unwrap_or_else(|| {
        // Try to find target from config.json
        if let Some(target) = lookup_typst_target_from_config(source_path) {
            return target;
        }
        // Fallback: same directory as source, with .pdf extension
        let stem = std::path::Path::new(source_path).file_stem().and_then(|s| s.to_str()).unwrap_or("output");
        format!("{}.pdf", stem)
    });

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

/// Look up the target PDF path for a source .typ file from config.json.
/// Reads .context-pilot/config.json → modules.typst.documents → find matching source → return target.
fn lookup_typst_target_from_config(source_path: &str) -> Option<String> {
    let config_path = std::path::Path::new(".context-pilot/config.json");
    let content = std::fs::read_to_string(config_path).ok()?;
    let root: serde_json::Value = serde_json::from_str(&content).ok()?;
    let documents = root.get("modules")?.get("typst")?.get("documents")?.as_object()?;

    for (_name, doc) in documents {
        if let Some(src) = doc.get("source").and_then(|v| v.as_str())
            && src == source_path
        {
            return doc.get("target").and_then(|v| v.as_str()).map(|s| s.to_string());
        }
    }
    None
}

/// Run the typst-compile-template subcommand: recompile all documents that use a given template.
/// Usage: cpilot typst-compile-template <template.typ>
///
/// Reads config.json → modules.typst.documents, finds all docs with matching template, compiles each.
fn run_typst_compile_template(args: &[String]) -> io::Result<()> {
    if args.is_empty() {
        eprintln!("Usage: cpilot typst-compile-template <template.typ>");
        std::process::exit(1);
    }

    let template_path = &args[0];

    // Extract template name from path (e.g., ".context-pilot/pdf/templates/report.typ" → "report")
    let template_name = std::path::Path::new(template_path).file_stem().and_then(|s| s.to_str()).unwrap_or("");

    if template_name.is_empty() {
        eprintln!("Could not extract template name from: {}", template_path);
        std::process::exit(1);
    }

    // Read config.json to find documents using this template
    let config_path = std::path::Path::new(".context-pilot/config.json");
    let content = match std::fs::read_to_string(config_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Cannot read config.json: {}", e);
            std::process::exit(1);
        }
    };
    let root: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Cannot parse config.json: {}", e);
            std::process::exit(1);
        }
    };

    let documents = match root
        .get("modules")
        .and_then(|m| m.get("typst"))
        .and_then(|t| t.get("documents"))
        .and_then(|d| d.as_object())
    {
        Some(d) => d,
        None => {
            println!("No typst documents found in config.json");
            return Ok(());
        }
    };

    let mut compiled = 0;
    let mut errors = 0;

    for (_name, doc) in documents {
        let doc_template = doc.get("template").and_then(|v| v.as_str()).unwrap_or("");
        if doc_template != template_name {
            continue;
        }

        let source = match doc.get("source").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => continue,
        };
        let target = doc.get("target").and_then(|v| v.as_str());

        // Write directly to target path (no intermediate output dir)
        let output = match target {
            Some(t) => t.to_string(),
            None => {
                let stem = std::path::Path::new(source).file_stem().and_then(|s| s.to_str()).unwrap_or("output");
                format!("{}.pdf", stem)
            }
        };

        match cp_mod_typst::compiler::compile_and_write(source, &output) {
            Ok(msg) => {
                println!("{}", msg);
                compiled += 1;
            }
            Err(err) => {
                eprintln!("Error compiling {}: {}", source, err);
                errors += 1;
            }
        }
    }

    if errors > 0 {
        eprintln!("Template '{}': {} compiled, {} failed", template_name, compiled, errors);
        std::process::exit(1);
    } else if compiled == 0 {
        println!("No documents use template '{}'", template_name);
    } else {
        println!("Template '{}': {} document(s) recompiled", template_name, compiled);
    }

    Ok(())
}
