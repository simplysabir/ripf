use ignore::WalkBuilder;

pub mod engine;
pub mod files;

/// Cap on results held in memory, shared by both engines. A query like `e`
/// would otherwise eat memory and stall rendering.
pub const MAX_HITS: usize = 5_000;

/// One matching *line* — not one match. A line with three matches yields one
/// Hit, with `col` taken from the first, same as `rg --column`.
#[derive(Debug, Clone)]
pub struct Hit {
    pub path: String,
    pub line_number: u64,
    pub col: u64,
    pub line_text: String,
}

/// Streamed from a worker thread to the UI thread. Every variant carries the
/// generation it belongs to so the UI can drop stale traffic.
#[derive(Debug)]
pub enum SearchMsg {
    /// A batch of results. Many of these arrive per search.
    Hits(u64, Vec<Hit>),
    /// The walk finished (or was cancelled). Carries elapsed milliseconds.
    Done(u64, u128),
    /// The search never started — bad regex, bad type filter.
    Error(u64, String),
}

/// Walk-level flags shared by both engines, straight from the CLI.
#[derive(Debug, Clone, Copy, Default)]
pub struct WalkOpts {
    pub hidden: bool,
    pub no_ignore: bool,
}

impl WalkOpts {
    /// Note the inversions: `ignore`'s builder takes "should I *apply* this
    /// filter", while our flags are "should I *bypass* it".
    pub fn apply(self, b: &mut WalkBuilder) {
        b.hidden(!self.hidden)
            .git_ignore(!self.no_ignore)
            .git_global(!self.no_ignore)
            .git_exclude(!self.no_ignore)
            .ignore(!self.no_ignore)
            .parents(!self.no_ignore);
    }
}
