use anyhow::{Context, Result};
use std::process::Command;

pub fn build_argv(template: &str, file: &str, line: u64, col: u64) -> Result<Vec<String>> {
    let tokens = shell_words::split(template)
        .with_context(|| format!("open_command is not valid shell syntax: `{template}`"))?;

    let argv: Vec<String> = tokens
        .iter()
        .map(|t| {
            t.replace("{file}", file)
                .replace("{line}", &line.to_string())
                .replace("{col}", &col.to_string())
        })
        .collect();

    if argv.is_empty() {
        anyhow::bail!("open_command is empty");
    }

    Ok(argv)
}

pub fn open(template: &str, file: &str, line: u64, col: u64) -> Result<()> {
    let argv = build_argv(template, file, line, col)?;

    let status = Command::new(&argv[0])
        .args(&argv[1..])
        .status()
        .with_context(|| format!("failed to run `{}`", argv[0]))?;

    if !status.success() {
        anyhow::bail!("`{}` exited with {status}", argv[0]);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subsitute_all_placeholders() {
        let argv = build_argv("cursor -g {file}:{line}:{col}", "src/main.rs", 42, 7).unwrap();
        assert_eq!(argv, vec!["cursor", "-g", "src/main.rs:42:7"]);
    }

    #[test]
    fn path_with_spaces_stays_one_arg() {
        let argv = build_argv("cursor -g {file}:{line}:{col}", "my dir/a.rs", 1, 1).unwrap();
        assert_eq!(argv, vec!["cursor", "-g", "my dir/a.rs:1:1"]);
    }

    #[test]
    fn template_without_col_is_valid() {
        let argv = build_argv("nvim +{line} {file}", "a.rs", 9, 3).unwrap();
        assert_eq!(argv, vec!["nvim", "+9", "a.rs"]);
    }

    #[test]
    fn empty_template_errors() {
        assert!(build_argv("", "a.rs", 1, 1).is_err());
    }
}
