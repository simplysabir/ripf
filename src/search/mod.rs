pub mod engine;

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
