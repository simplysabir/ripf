//! End-to-end tests for the native search engine, driven through `--print`.
//!
//! The TUI is deliberately untested — you're the CI for that. This covers the
//! engine: walking, ignore rules, smart-case, type filters, exit codes.
//!
//! NOTE: `tests/fixtures/repo/build/out.txt` is excluded by the fixture's own
//! .gitignore, so `git add -A` skips it and a fresh clone would make
//! `skips_gitignored_and_hidden_files` pass vacuously. It must be committed
//! with `git add -f tests/fixtures/repo/build/out.txt`. (The `ignore` crate
//! applies gitignore rules textually and never consults git's index, so a
//! force-tracked file is still correctly excluded from search.)

use std::path::PathBuf;
use std::process::Command;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/repo")
}

/// `CARGO_BIN_EXE_<name>` is set by cargo for integration tests and points at
/// the freshly built binary, so these always test current code.
fn ripf() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ripf"));
    cmd.current_dir(fixture());
    cmd
}

fn run(args: &[&str]) -> (Vec<String>, Option<i32>, String) {
    let out = ripf().args(args).output().expect("failed to run ripf");
    let lines = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_string)
        .collect();
    (
        lines,
        out.status.code(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn finds_matches_across_file_types() {
    let (lines, code, _) = run(&["--print", "needle"]);
    assert_eq!(code, Some(0));
    assert_eq!(
        lines,
        ["conf.toml:1:1", "src/main.rs:2:9", "src/main.rs:3:15"]
    );
}

#[test]
fn output_is_sorted_despite_the_parallel_walk() {
    // Run twice: a parallel walk that leaked ordering would eventually differ.
    let (a, _, _) = run(&["--print", "needle"]);
    let (b, _, _) = run(&["--print", "needle"]);
    assert_eq!(a, b);
}

#[test]
fn skips_gitignored_and_hidden_files() {
    let (lines, _, _) = run(&["--print", "needle"]);
    assert!(
        !lines.iter().any(|l| l.contains("build/out.txt")),
        "gitignored file was searched: {lines:?}"
    );
    assert!(
        !lines.iter().any(|l| l.contains("hidden")),
        "hidden file was searched: {lines:?}"
    );
}

#[test]
fn smart_case_lowercase_query_is_insensitive() {
    let (lines, _, _) = run(&["--print", "needle"]);
    // Matches both `needle` and `Needle`.
    assert!(lines.contains(&"src/main.rs:3:15".to_string()));
}

#[test]
fn smart_case_uppercase_query_is_sensitive() {
    let (lines, _, _) = run(&["--print", "Needle"]);
    assert_eq!(lines, ["src/main.rs:3:15"]);
}

#[test]
fn type_filter_restricts_results() {
    let (lines, _, _) = run(&["--print", "-t", "toml", "needle"]);
    assert_eq!(lines, ["conf.toml:1:1"]);
}

#[test]
fn no_matches_exits_one_like_grep() {
    let (lines, code, _) = run(&["--print", "zzzznope"]);
    assert!(lines.is_empty());
    assert_eq!(code, Some(1));
}

#[test]
fn invalid_regex_exits_two_with_a_message() {
    let (_, code, stderr) = run(&["--print", "foo("]);
    assert_eq!(code, Some(2));
    assert!(
        stderr.contains("regex parse error"),
        "unhelpful error: {stderr}"
    );
}

// --- file mode (-f) ---

#[test]
fn file_mode_lists_every_searchable_file() {
    let (mut lines, code, _) = run(&["--print", "-f"]);
    lines.sort();
    assert_eq!(code, Some(0));
    assert_eq!(lines, ["conf.toml", "src/main.rs"]);
}

#[test]
fn file_mode_respects_the_same_ignore_rules() {
    let (lines, _, _) = run(&["--print", "-f"]);
    assert!(
        !lines.iter().any(|l| l.contains("build/out.txt")),
        "gitignored file listed: {lines:?}"
    );
    assert!(
        !lines.iter().any(|l| l.contains("hidden")),
        "hidden file listed: {lines:?}"
    );
}

#[test]
fn file_mode_fuzzy_matches_non_contiguous_characters() {
    // 'mainrs' appears nowhere as a substring; it only matches fuzzily.
    let (lines, _, _) = run(&["--print", "-f", "mainrs"]);
    assert_eq!(lines, ["src/main.rs"]);
}

#[test]
fn file_mode_ranks_best_match_first() {
    // Both files match 'o'; the ranking must be deterministic, not walk order.
    let (a, _, _) = run(&["--print", "-f", "o"]);
    let (b, _, _) = run(&["--print", "-f", "o"]);
    assert_eq!(a, b, "ranking is unstable between runs");
    assert!(!a.is_empty());
}

#[test]
fn file_mode_no_match_exits_one() {
    let (lines, code, _) = run(&["--print", "-f", "zzzqqqnope"]);
    assert!(lines.is_empty());
    assert_eq!(code, Some(1));
}

// --- ignore overrides ---

#[test]
fn hidden_flag_includes_dotfiles() {
    let (without, _, _) = run(&["--print", "needle"]);
    let (with, _, _) = run(&["--print", "--hidden", "needle"]);
    assert!(!without.iter().any(|l| l.contains(".hidden.txt")));
    assert!(
        with.iter().any(|l| l.contains(".hidden.txt")),
        "--hidden did not reach the dotfile: {with:?}"
    );
}

#[test]
fn no_ignore_flag_includes_gitignored_files() {
    let (without, _, _) = run(&["--print", "needle"]);
    let (with, _, _) = run(&["--print", "--no-ignore", "needle"]);
    assert!(!without.iter().any(|l| l.contains("build/out.txt")));
    assert!(
        with.iter().any(|l| l.contains("build/out.txt")),
        "--no-ignore did not reach the ignored file: {with:?}"
    );
}

#[test]
fn ignore_overrides_apply_in_file_mode_too() {
    let (with, _, _) = run(&["--print", "-f", "--hidden", "--no-ignore"]);
    assert!(with.iter().any(|l| l.contains(".hidden.txt")));
    assert!(with.iter().any(|l| l.contains("build/out.txt")));
}

#[test]
fn query_starting_with_a_dash_is_not_a_flag() {
    let (_, code, stderr) = run(&["--print", "--", "-needle"]);
    // No match here, but it must not be a *parse* failure.
    assert_ne!(code, Some(2), "treated as a flag: {stderr}");
}
