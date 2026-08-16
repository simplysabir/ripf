use anyhow::{Context, Result};
use serde::Deserialize;
use serde::de::IgnoredAny;
use std::io::{BufRead, BufReader, Read};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

/// One matching *line* — not one match. A line with three matches yields one
/// Hit, with `col` taken from the first, same as `rg --column`.
#[derive(Debug, Clone)]
pub struct Hit {
    pub path: String,
    pub line_number: u64,
    pub col: u64,
    pub line_text: String,
}

#[derive(Deserialize, Debug)]
struct Text {
    text: String,
}

#[derive(Deserialize, Debug)]
struct SubMatch {
    start: u64,
}

#[derive(Deserialize, Debug)]
struct MatchData {
    path: Text,
    lines: Text,
    line_number: Option<u64>,
    submatches: Vec<SubMatch>,
}

/// `rg --json` is *adjacently* tagged: discriminant in "type", payload in
/// "data" — that's serde's `tag = ..., content = ...`. (Internally tagged
/// would mean the payload fields sit alongside "type" at the top level.)
///
/// The variants we don't care about carry `IgnoredAny`, which consumes and
/// discards the payload without allocating it.
#[derive(Deserialize, Debug)]
#[serde(tag = "type", content = "data", rename_all = "lowercase")]
enum RgMessage {
    Match(MatchData),
    Begin(IgnoredAny),
    End(IgnoredAny),
    Summary(IgnoredAny),
    Context(IgnoredAny),
}

/// Lines we can't parse are skipped, never fatal. That covers two real cases:
/// a future rg version emitting a message type we don't know, and non-UTF-8
/// paths (rg emits {"bytes": "<base64>"} instead of {"text": ...}, which
/// fails this schema). A dropped hit beats a dead search.
fn parse_line(line: &str) -> Option<Hit> {
    match serde_json::from_str::<RgMessage>(line).ok()? {
        RgMessage::Match(m) => Some(Hit {
            path: m.path.text,
            line_number: m.line_number?,
            // +1 because editors are 1-indexed. NOTE: `start` is a 0-indexed
            // *byte* offset into the line, not a character offset. Identical
            // for ASCII; a match after a multibyte char lands a few columns
            // off. Acceptable for v1.
            col: m.submatches.first().map_or(0, |s| s.start) + 1,
            line_text: m.lines.text.trim_end().to_string(),
        }),
        _ => None,
    }
}

/// Cap so a query like `e` can't eat memory or stall rendering.
pub const MAX_HITS: usize = 5_000;

pub fn search(
    query: &str,
    types: &[String],
    my_gen: u64,
    current_gen: &AtomicU64,
) -> Result<Vec<Hit>> {
    let mut cmd = Command::new("rg");
    cmd.arg("--json").arg("--color=never");
    for t in types {
        cmd.arg("--type").arg(t);
    }
    // `--` so a query starting with `-` isn't parsed as a flag by rg.
    cmd.arg("--").arg(query);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .context("failed to run `rg` — is ripgrep installed? (brew install ripgrep)")?;

    let stdout = child.stdout.take().expect("stdout was piped above");
    let mut hits: Vec<Hit> = Vec::new();

    for (i, line) in BufReader::new(stdout).lines().enumerate() {
        let Ok(line) = line else { break };

        // Cooperative cancellation: the UI thread bumps `current_gen` on every
        // keystroke, and we notice on our next read. Checked every 64 lines so
        // an atomic load isn't in the hot path.
        if i % 64 == 0 && current_gen.load(Ordering::Relaxed) != my_gen {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(Vec::new());
        }

        if let Some(hit) = parse_line(&line) {
            hits.push(hit);
            if hits.len() >= MAX_HITS {
                let _ = child.kill();
                let _ = child.wait();
                return Ok(hits);
            }
        }
    }

    // Reap the child so it isn't left a zombie.
    let status = child.wait().context("failed to wait on rg")?;

    // rg: 0 = matches, 1 = no matches (a normal outcome), 2 = real failure
    // such as an invalid regex. Only 2 is worth surfacing.
    if status.code() == Some(2) {
        let mut err = String::new();
        if let Some(mut s) = child.stderr.take() {
            let _ = s.read_to_string(&mut err);
        }
        anyhow::bail!("rg failed: {}", err.trim());
    }

    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Real `rg --json` output, shortened so each const fits on one line.
    // Payloads for begin/end are IgnoredAny, so their contents don't matter.
    // Split with concat! purely so no source line is long enough to wrap;
    // concat! joins string literals at compile time, so MATCH is one &str.
    const MATCH: &str = concat!(
        r#"{"type":"match","data":{"#,
        r#""path":{"text":"a.rs"},"#,
        r#""lines":{"text":"let col = 1;\n"},"#,
        r#""line_number":1,"absolute_offset":0,"#,
        r#""submatches":[{"match":{"text":"col"},"start":4,"end":7}]"#,
        r#"}}"#,
    );
    const BEGIN: &str = r#"{"type":"begin","data":{"path":{"text":"a.rs"}}}"#;
    const END: &str = r#"{"type":"end","data":{"path":{"text":"a.rs"},"stats":{"matches":1}}}"#;

    #[test]
    fn parses_a_match() {
        let hit = parse_line(MATCH).expect("should parse");
        assert_eq!(hit.path, "a.rs");
        assert_eq!(hit.line_number, 1);
        assert_eq!(hit.col, 5); // byte offset 4, 1-indexed
        assert_eq!(hit.line_text, "let col = 1;");
    }

    #[test]
    fn ignores_non_match_messages() {
        assert!(parse_line(BEGIN).is_none());
        assert!(parse_line(END).is_none());
    }

    #[test]
    fn skips_unknown_message_types() {
        assert!(parse_line(r#"{"type":"futuretype","data":{"x":true}}"#).is_none());
    }

    #[test]
    fn skips_non_utf8_paths() {
        let line = concat!(
            r#"{"type":"match","data":{"path":{"bytes":"aGVsbG8="},"#,
            r#""lines":{"text":"hi\n"},"line_number":1,"absolute_offset":0,"#,
            r#""submatches":[{"match":{"text":"h"},"start":0,"end":1}]}}"#,
        );
        assert!(parse_line(line).is_none());
    }

    #[test]
    fn skips_garbage() {
        assert!(parse_line("not json at all").is_none());
        assert!(parse_line("").is_none());
    }
}
