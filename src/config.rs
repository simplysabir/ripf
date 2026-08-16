use crate::cli::Cli;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::{
    format,
    path::{PathBuf},
};

/// Mirrors config.toml exactly. Every field optional so a partial file works.
#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub open_command: Option<String>,

    /// Option, not bool: `#[derive(Default)]` would make a bare bool `false`,
    /// which is the wrong default. Resolved to `true` in `Settings::resolve`.
    pub quit_on_open: Option<bool>,
}

/// XDG-style, deliberately NOT dirs::config_dir(): on macOS that returns
/// ~/Library/Application Support, but ripf is a terminal tool and its config
/// belongs next to nvim/gh/git in ~/.config.
fn config_path() -> Option<PathBuf> {
    let base = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(x) if !x.is_empty() => PathBuf::from(x),
        _ => dirs::home_dir()?.join(".config"),
    };
    Some(base.join("ripf").join("config.toml"))
}

pub fn load() -> Result<Config> {
    let Some(path) = config_path() else {
        return Ok(Config::default());
    };

    match std::fs::read_to_string(&path) {
        Ok(text) => toml::from_str(&text)
            .with_context(|| format!("failed to parse config file: `{}`", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
        Err(e) => {
            Err(e).with_context(|| format!("failed to read config file: `{}`", path.display()))
        }
    }
}

/// Precedence: --open > config file > $EDITOR.
///
/// Pure by design: reading $EDITOR is the caller's job, so this is testable.
/// (In edition 2024 `std::env::set_var` is `unsafe` — tests share one process
/// environment and run on parallel threads, so env-dependent logic can't be
/// unit tested safely.)
fn pick_open_command(
    cli_open: Option<String>,
    config_open: Option<String>,
    editor: Option<String>,
) -> Option<String> {
    cli_open.or(config_open).or_else(|| {
        let editor = editor?;
        let editor = editor.trim();
        if editor.is_empty() {
            return None;
        }
        // Shortcut: drops the line number, per the contract in the plan.
        // `format!("{editor} +{{line}} {{file}}")` would keep it for vim/nvim
        // but break `code`/`cursor` used as $EDITOR.
        Some(format!("{editor} {{file}}"))
    })
}

/// CLI flags and config file merged into one resolved view
#[derive(Debug)]
pub struct Settings {
    /// `None` is only reachable when `print` is true - enforced in `resolve`
    pub open_command: Option<String>,
    pub query: Option<String>,
    pub print: bool,
    pub types: Vec<String>,
    pub quit_on_open: bool,
}

impl Settings {
    pub fn resolve(cli: Cli, config: Config) -> Result<Self> {
        let open_command =
            pick_open_command(cli.open, config.open_command, std::env::var("EDITOR").ok());

        if open_command.is_none() && !cli.print {
            anyhow::bail!(
                "no editor configured.\n  \
                 set open_command in {}\n  \
                 e.g. open_command = \"cursor -g {{file}}:{{line}}:{{col}}\"\n  \
                 or pass --open \"...\", or set $EDITOR",
                config_path()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "~/.config/ripf/config.toml".into())
            );
        }

        Ok(Settings {
            open_command,
            query: cli.query,
            print: cli.print,
            types: cli.types,
            quit_on_open: config.quit_on_open.unwrap_or(true),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_beats_config_and_editor() {
        let got = pick_open_command(
            Some("a {file}".into()),
            Some("b {file}".into()),
            Some("vim".into()),
        );
        assert_eq!(got.as_deref(), Some("a {file}"));
    }

    #[test]
    fn config_beats_editor() {
        let got = pick_open_command(None, Some("b {file}".into()), Some("vim".into()));
        assert_eq!(got.as_deref(), Some("b {file}"));
    }

    #[test]
    fn editor_is_last_resort() {
        let got = pick_open_command(None, None, Some("vim".into()));
        assert_eq!(got.as_deref(), Some("vim {file}"));
    }

    #[test]
    fn blank_or_absent_editor_is_no_fallback() {
        assert_eq!(pick_open_command(None, None, Some("  ".into())), None);
        assert_eq!(pick_open_command(None, None, None), None);
    }
}
