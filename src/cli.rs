use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "ripf",
    version,
    about = "Interactive code search that opens your editor at the match"
)]
pub struct Cli {
    /// Search query (regex). Omit to open the TUI with an empty query
    pub query: Option<String>,

    /// Command used to open a result, e.g. "cursor -g {file}:{line}:{col}".
    /// Overrides config.
    #[arg(long)]
    pub open: Option<String>,

    /// Print file:line:col matches to stdout instead of opening an editor
    #[arg(long)]
    pub print: bool,

    /// Restrict to a file type, e.g. -t rust. Repeatable
    #[arg(short = 't', long = "type")]
    pub types: Vec<String>,
}
