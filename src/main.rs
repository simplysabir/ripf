mod app;
mod cli;
mod config;
mod open;
mod rg;
mod tui;
mod ui;

use crate::app::App;
use crate::cli::Cli;
use crate::config::Settings;
use crate::rg::Hit;
use crate::tui::TerminalGuard;
use anyhow::Result;
use clap::Parser;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::time::{Duration, Instant};

/// How long the query must be still before a search fires.
const DEBOUNCE: Duration = Duration::from_millis(80);
/// How long to wait for a key before looping to redraw and drain results.
const POLL: Duration = Duration::from_millis(30);

type SearchResult = (u64, Result<Vec<Hit>, String>, u128);

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("ripf: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<ExitCode> {
    let cli = Cli::parse();
    let config = config::load()?;
    let settings = Settings::resolve(cli, config)?;

    if settings.print {
        return run_print(&settings);
    }
    run_tui(&settings)?;
    Ok(ExitCode::SUCCESS)
}

fn run_print(settings: &Settings) -> Result<ExitCode> {
    let Some(query) = settings.query.as_deref() else {
        anyhow::bail!("--print needs a query");
    };

    // No cancellation here: one search, run to completion. Passing a counter
    // that equals `my_gen` means the staleness check never fires.
    let never = AtomicU64::new(0);
    let hits = rg::search(query, &settings.types, 0, &never)?;

    if hits.is_empty() {
        return Ok(ExitCode::from(1));
    }
    for h in &hits {
        println!("{}:{}:{}", h.path, h.line_number, h.col);
    }
    Ok(ExitCode::SUCCESS)
}

fn run_tui(settings: &Settings) -> Result<()> {
    let template = settings
        .open_command
        .as_deref()
        .expect("resolve() guarantees Some when !print");

    let mut app = App::new(settings.query.clone().unwrap_or_default());
    let current_gen = Arc::new(AtomicU64::new(0));
    let (tx, rx) = mpsc::channel::<SearchResult>();

    let mut guard = TerminalGuard::new()?;

    // Search immediately if a query came in on the command line.
    let mut pending = !app.query.is_empty();
    let mut last_edit = Instant::now();

    loop {
        guard.terminal().draw(|f| ui::draw(f, &app))?;

        if pending && last_edit.elapsed() >= DEBOUNCE {
            pending = false;
            spawn_search(&app, settings, &current_gen, &tx);
        }

        // Non-blocking drain: never wait on the worker from the UI thread.
        while let Ok((generation, res, ms)) = rx.try_recv() {
            match res {
                Ok(hits) => app.set_hits(generation, hits, ms),
                Err(e) => app.set_error(generation, e),
            }
        }

        if event::poll(POLL)? {
            if let Event::Key(KeyEvent {
                code,
                modifiers,
                kind: KeyEventKind::Press,
                ..
            }) = event::read()?
            {
                let mut edited = false;
                match (code, modifiers) {
                    (KeyCode::Esc, _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                        app.should_quit = true;
                    }
                    (KeyCode::Down, _) | (KeyCode::Char('j'), KeyModifiers::CONTROL) => {
                        app.move_selection(1);
                    }
                    (KeyCode::Up, _) | (KeyCode::Char('k'), KeyModifiers::CONTROL) => {
                        app.move_selection(-1);
                    }
                    (KeyCode::Tab, _) => app.toggle_mark(),
                    (KeyCode::Left, _) => app.move_left(),
                    (KeyCode::Right, _) => app.move_right(),
                    (KeyCode::Backspace, _) => {
                        app.backspace();
                        edited = true;
                    }
                    (KeyCode::Enter, _) => {
                        open_targets(&mut app, &mut guard, template, settings.quit_on_open)?;
                    }
                    // Must come after the ctrl- arms above, and must exclude
                    // CONTROL, or ctrl-j would type a literal 'j'.
                    (KeyCode::Char(c), m) if m.is_empty() || m == KeyModifiers::SHIFT => {
                        app.insert(c);
                        edited = true;
                    }
                    _ => {}
                }

                if edited {
                    // Bump immediately, not when the search fires: any in-flight
                    // worker sees the new value on its next check and gives up.
                    app.generation += 1;
                    current_gen.store(app.generation, Ordering::Relaxed);
                    pending = true;
                    last_edit = Instant::now();
                }
            }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

fn spawn_search(
    app: &App,
    settings: &Settings,
    current_gen: &Arc<AtomicU64>,
    tx: &Sender<SearchResult>,
) {
    let generation = app.generation;

    // An empty query means empty results, not "search for nothing".
    if app.query.is_empty() {
        let _ = tx.send((generation, Ok(Vec::new()), 0));
        return;
    }

    let query = app.query.clone();
    let types = settings.types.clone();
    let current_gen = Arc::clone(current_gen);
    let tx = tx.clone();

    std::thread::spawn(move || {
        let started = Instant::now();
        let res = rg::search(&query, &types, generation, &current_gen).map_err(|e| format!("{e:#}"));
        // Send failing just means the UI quit; nothing to do about it.
        let _ = tx.send((generation, res, started.elapsed().as_millis()));
    });
}

fn open_targets(
    app: &mut App,
    guard: &mut TerminalGuard,
    template: &str,
    quit_on_open: bool,
) -> Result<()> {
    // Collect owned data first: `targets()` borrows `app`, and we mutate it below.
    let targets: Vec<(String, u64, u64)> = app
        .targets()
        .iter()
        .map(|h| (h.path.clone(), h.line_number, h.col))
        .collect();

    if targets.is_empty() {
        return Ok(());
    }

    // Hand the terminal back so a TUI editor (nvim, hx) can take it over.
    guard.suspend()?;

    let mut error = None;
    for (path, line, col) in &targets {
        if let Err(e) = open::open(template, path, *line, *col) {
            error = Some(format!("{e:#}"));
            break;
        }
    }

    if quit_on_open && error.is_none() {
        app.should_quit = true; // the guard's Drop restores the terminal
    } else {
        guard.resume()?;
        if let Some(e) = error {
            app.status = e;
        }
    }
    Ok(())
}
