use crate::search::{Hit, SearchMsg};
use grep_matcher::Matcher;
use grep_regex::{RegexMatcher, RegexMatcherBuilder};
use grep_searcher::{BinaryDetection, Searcher, SearcherBuilder, Sink, SinkMatch};
use ignore::types::{Types, TypesBuilder};
use ignore::{WalkBuilder, WalkState};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::time::Instant;

/// Cap so a query like `e` can't eat memory or stall rendering.
pub const MAX_HITS: usize = 5_000;
/// Hits per channel message. One send per hit would swamp the UI thread.
const BATCH: usize = 50;

/// Receives matches from `grep-searcher` for a single file.
struct HitSink<'a> {
    path: String,
    matcher: &'a RegexMatcher,
    generation: u64,
    current_gen: &'a AtomicU64,
    total: &'a AtomicUsize,
    tx: &'a Sender<SearchMsg>,
    batch: Vec<Hit>,
}

impl HitSink<'_> {
    fn flush(&mut self) {
        if !self.batch.is_empty() {
            let batch = std::mem::take(&mut self.batch);
            let _ = self.tx.send(SearchMsg::Hits(self.generation, batch));
        }
    }
}

impl Sink for HitSink<'_> {
    type Error = std::io::Error;

    fn matched(&mut self, _searcher: &Searcher, m: &SinkMatch<'_>) -> Result<bool, std::io::Error> {
        // `Ok(false)` tells grep-searcher to stop searching *this file*.
        if self.current_gen.load(Ordering::Relaxed) != self.generation {
            return Ok(false);
        }
        if self.total.fetch_add(1, Ordering::Relaxed) >= MAX_HITS {
            return Ok(false);
        }

        let bytes = m.bytes();
        // Byte offset of the first match within the line, 1-indexed for editors.
        let col = self
            .matcher
            .find(bytes)
            .ok()
            .flatten()
            .map_or(0, |mm| mm.start()) as u64
            + 1;

        self.batch.push(Hit {
            path: self.path.clone(),
            line_number: m.line_number().unwrap_or(0),
            col,
            line_text: String::from_utf8_lossy(bytes).trim_end().to_string(),
        });

        if self.batch.len() >= BATCH {
            self.flush();
        }
        Ok(true)
    }
}

fn build_matcher(query: &str) -> Result<RegexMatcher, String> {
    RegexMatcherBuilder::new()
        // Case-insensitive unless the query contains an uppercase character.
        .case_smart(true)
        .line_terminator(Some(b'\n'))
        .build(query)
        .map_err(|e| first_line(&e.to_string()))
}

fn build_types(selected: &[String]) -> Result<Types, String> {
    let mut builder = TypesBuilder::new();
    // ripgrep's own type definitions — `rust`, `py`, `toml`, ~200 more.
    builder.add_defaults();
    for t in selected {
        builder.select(t);
    }
    builder.build().map_err(|e| first_line(&e.to_string()))
}

/// The status bar is one line; these errors are several.
fn first_line(s: &str) -> String {
    s.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("search failed")
        .to_string()
}

/// Runs to completion on the calling thread, streaming batches over `tx`.
/// Call it from a worker thread, never from the UI thread.
pub fn search(
    query: &str,
    types: &[String],
    generation: u64,
    current_gen: Arc<AtomicU64>,
    tx: Sender<SearchMsg>,
) {
    let started = Instant::now();

    let matcher = match build_matcher(query) {
        Ok(m) => m,
        Err(e) => {
            let _ = tx.send(SearchMsg::Error(generation, e));
            return;
        }
    };
    let types = match build_types(types) {
        Ok(t) => t,
        Err(e) => {
            let _ = tx.send(SearchMsg::Error(generation, e));
            return;
        }
    };

    let total = Arc::new(AtomicUsize::new(0));

    WalkBuilder::new(".")
        .types(types)
        .build_parallel()
        // Called once per walker thread; whatever it returns visits entries.
        .run(|| {
            let matcher = matcher.clone();
            let tx = tx.clone();
            let total = Arc::clone(&total);
            let current_gen = Arc::clone(&current_gen);
            let mut searcher = SearcherBuilder::new()
                .line_number(true)
                .binary_detection(BinaryDetection::quit(0))
                .build();

            Box::new(move |entry| {
                if current_gen.load(Ordering::Relaxed) != generation
                    || total.load(Ordering::Relaxed) >= MAX_HITS
                {
                    // Quit stops every walker thread, not just this one.
                    return WalkState::Quit;
                }

                let Ok(entry) = entry else {
                    return WalkState::Continue;
                };
                if !entry.file_type().is_some_and(|t| t.is_file()) {
                    return WalkState::Continue;
                }

                let path = entry.path();
                // The walk root is "." so every path comes back as "./src/x".
                // Strip it: editors and `--print` consumers want "src/x".
                let display = path.strip_prefix("./").unwrap_or(path);
                let mut sink = HitSink {
                    path: display.to_string_lossy().into_owned(),
                    matcher: &matcher,
                    generation,
                    current_gen: &current_gen,
                    total: &total,
                    tx: &tx,
                    batch: Vec::new(),
                };

                // Unreadable file, permission denied, invalid UTF-16: skip it.
                let _ = searcher.search_path(&matcher, path, &mut sink);
                sink.flush();

                WalkState::Continue
            })
        });

    let _ = tx.send(SearchMsg::Done(generation, started.elapsed().as_millis()));
}
