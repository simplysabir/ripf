//! A window of file contents around the selected match.
//!
//! Deliberately unhighlighted: no `bat` subprocess (we removed the last one in
//! 0.2.0) and no syntect (a few MB of syntax definitions for colour). If that
//! changes, only `load` needs to grow — the rest of the app just renders
//! `Vec<(line_number, text)>`.

use std::fs::File;
use std::io::{BufRead, BufReader};

/// Lines loaded around the match. Generous enough to fill a tall pane.
const WINDOW: u64 = 60;
/// Tabs render unpredictably inside a bordered pane; expand them ourselves.
const TAB: &str = "    ";

pub struct Preview {
    pub path: String,
    /// The line the match is on, so the renderer can mark it.
    pub line: u64,
    pub lines: Vec<(u64, String)>,
}

/// Returns `None` for unreadable paths. Binary files stop early rather than
/// erroring — you get whatever decoded cleanly, which is usually nothing.
pub fn load(path: &str, line: u64) -> Option<Preview> {
    let file = File::open(path).ok()?;

    // Centre the window on the match, but never start before line 1.
    let start = line.saturating_sub(WINDOW / 3).max(1);
    let end = start + WINDOW;

    let mut lines = Vec::new();
    for (i, text) in BufReader::new(file).lines().enumerate() {
        let n = i as u64 + 1;
        if n < start {
            continue;
        }
        if n >= end {
            break;
        }
        // map_while semantics: invalid UTF-8 ends the preview, not the program.
        let Ok(text) = text else { break };
        lines.push((n, text.replace('\t', TAB)));
    }

    Some(Preview {
        path: path.to_string(),
        line,
        lines,
    })
}
