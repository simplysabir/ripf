mod app;
mod cli;
mod config;
mod keys;
mod open;
mod preview;
mod search;
mod state;
mod tui;
mod ui;

use crate::app::{App, Mode};
use crate::cli::Cli;
use crate::config::Settings;
use crate::search::{MAX_HITS, SearchMsg};
use crate::search::{engine, files};
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
    let query = settings.query.as_deref().unwrap_or("");
    // GREP has nothing to search for without a query; FILES lists everything.
    if query.is_empty() && !settings.files {
        anyhow::bail!("--print needs a query");
    }

    let (tx, rx) = mpsc::channel::<SearchMsg>();
    // Generation 0 throughout, so the staleness check never fires.
    let generation = Arc::new(AtomicU64::new(0));
    if settings.files {
        files::search(
            query,
            settings.walk,
            usize::MAX,
            0,
            generation,
            files::empty_cache(),
            tx,
        );
    } else {
        engine::search(
            query,
            &settings.types,
            settings.walk,
            usize::MAX,
            0,
            generation,
            tx,
        );
    }

    let mut hits = Vec::new();
    // The iterator ends when every sender has dropped, which the search
    // functions guarantee by returning.
    for msg in rx {
        match msg {
            SearchMsg::Hits(_, batch) => hits.extend(batch),
            SearchMsg::Done(_, _) => {}
            SearchMsg::Error(_, e) => anyhow::bail!("{e}"),
        }
    }

    if hits.is_empty() {
        return Ok(ExitCode::from(1));
    }

    if settings.files {
        // Already ordered best-match-first; sorting would destroy the ranking.
        for h in &hits {
            println!("{}", h.path);
        }
    } else {
        // The walk is parallel, so arrival order is nondeterministic. Sort so
        // piping into diff/wc/tests gives stable output.
        hits.sort_by(|a, b| a.path.cmp(&b.path).then(a.line_number.cmp(&b.line_number)));
        for h in &hits {
            println!("{}:{}:{}", h.path, h.line_number, h.col);
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn run_tui(settings: &Settings) -> Result<()> {
    let template = settings
        .open_command
        .as_deref()
        .expect("resolve() guarantees Some when !print");

    // -r restores query/mode/cursor, but an explicit CLI query or -f still wins.
    let saved = settings.resume.then(state::load).flatten();
    let mode = match (settings.files, &saved) {
        (true, _) => Mode::Files,
        (false, Some(s)) => s.mode,
        (false, None) => Mode::Grep,
    };
    let query = settings
        .query
        .clone()
        .or_else(|| saved.as_ref().map(|s| s.query.clone()))
        .unwrap_or_default();

    let mut app = App::new(query, mode);
    app.restore_selected = saved.as_ref().map(|s| s.selected);
    let current_gen = Arc::new(AtomicU64::new(0));
    let file_cache = files::empty_cache();
    let (tx, rx) = mpsc::channel::<SearchMsg>();

    let mut guard = TerminalGuard::new()?;

    // Search immediately if a query came in on the command line.
    let mut pending = !app.query.is_empty() || app.mode == Mode::Files;
    let mut last_edit = Instant::now();

    loop {
        app.refresh_preview();
        guard.terminal().draw(|f| ui::draw(f, &app))?;

        if pending && last_edit.elapsed() >= DEBOUNCE {
            pending = false;
            app.status = "searching…".to_string();
            spawn_search(&app, settings, &current_gen, &file_cache, &tx);
        }

        // Non-blocking drain: never wait on the worker from the UI thread.
        while let Ok(msg) = rx.try_recv() {
            match msg {
                SearchMsg::Hits(g, batch) => app.append_hits(g, batch),
                SearchMsg::Done(g, ms) => app.finish(g, ms),
                SearchMsg::Error(g, e) => app.set_error(g, e),
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
                let km = &settings.keys;

                // Rebindable actions first: they're runtime values, so they
                // can't be `match` patterns.
                if km.toggle_mode.matches(code, modifiers) {
                    app.toggle_mode();
                    edited = true;
                } else if km.toggle_preview.matches(code, modifiers) {
                    app.show_preview = !app.show_preview;
                } else if km.refresh.matches(code, modifiers) {
                    // Drop the cached file list; the next FILES search rebuilds it.
                    *file_cache.lock().expect("file cache mutex poisoned") = None;
                    edited = true;
                } else if km.mark.matches(code, modifiers) {
                    app.toggle_mark();
                } else {
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
            state::save(&state::State {
                query: app.query.clone(),
                mode: app.mode,
                selected: app.selected,
            });
            return Ok(());
        }
    }
}

fn spawn_search(
    app: &App,
    settings: &Settings,
    current_gen: &Arc<AtomicU64>,
    file_cache: &files::SharedCache,
    tx: &Sender<SearchMsg>,
) {
    let generation = app.generation;

    // In GREP an empty query means empty results, not "search for nothing".
    // In FILES it means "every file", which is a useful thing to show.
    if app.query.is_empty() && app.mode == Mode::Grep {
        let _ = tx.send(SearchMsg::Done(generation, 0));
        return;
    }

    let query = app.query.clone();
    let types = settings.types.clone();
    let walk = settings.walk;
    let mode = app.mode;
    let current_gen = Arc::clone(current_gen);
    let file_cache = Arc::clone(file_cache);
    let tx = tx.clone();

    std::thread::spawn(move || match mode {
        Mode::Grep => engine::search(&query, &types, walk, MAX_HITS, generation, current_gen, tx),
        Mode::Files => files::search(
            &query,
            walk,
            MAX_HITS,
            generation,
            current_gen,
            file_cache,
            tx,
        ),
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
