mod app;
mod cli;
mod config;
mod open;
mod rg;
mod tui;

use crate::cli::Cli;
use crate::config::Settings;
use anyhow::Result;
use clap::Parser;
use std::io::{self, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(e) => {
            // `{:#}` is anyhow's alternate form: prints the whole context
            // chain on one line, e.g. "failed to parse X: expected `=`".
            eprintln!("ripf: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<ExitCode> {
    let cli = Cli::parse();
    let config = config::load()?;
    let settings = Settings::resolve(cli, config)?;

    let Some(query) = settings.query.as_deref() else {
        anyhow::bail!("a query is required for now — the TUI arrives in phase 2");
    };

    let hits = rg::search(query, &settings.types)?;

    // grep convention: exit 1 when there were no matches.
    if hits.is_empty() {
        return Ok(ExitCode::from(1));
    }

    if settings.print {
        for h in &hits {
            println!("{}:{}:{}", h.path, h.line_number, h.col);
        }
        return Ok(ExitCode::SUCCESS);
    }

    for (i, h) in hits.iter().enumerate() {
        println!(
            "[{}] {}:{}: {}",
            i + 1,
            h.path,
            h.line_number,
            h.line_text.trim()
        );
    }

    let Some(idx) = prompt_choice(hits.len())? else {
        return Ok(ExitCode::SUCCESS);
    };

    let hit = &hits[idx];
    let template = settings
        .open_command
        .as_deref()
        .expect("resolve() guarantees Some when !print");

    open::open(template, &hit.path, hit.line_number, hit.col)?;
    Ok(ExitCode::SUCCESS)
}

/// `None` means the user declined: empty line, EOF, or anything unparseable.
fn prompt_choice(n: usize) -> Result<Option<usize>> {
    // stderr so `ripf foo > out.txt` still shows the prompt.
    eprint!("open [1-{n}] (enter to cancel): ");
    io::stderr().flush()?;

    let mut line = String::new();
    if io::stdin().read_line(&mut line)? == 0 {
        return Ok(None); // EOF — ctrl-d, or stdin is a closed pipe
    }

    match line.trim().parse::<usize>() {
        Ok(choice) if (1..=n).contains(&choice) => Ok(Some(choice - 1)),
        _ => Ok(None),
    }
}
