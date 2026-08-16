use crate::search::{Hit, SearchMsg, WalkOpts};
use ignore::WalkBuilder;
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32String};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Hits per channel message, same reasoning as the grep engine.
const BATCH: usize = 50;

/// Every file in the tree, once. `haystacks[i]` is the UTF-32 form of
/// `paths[i]` — nucleo scores by char index, so converting here keeps the
/// conversion off the per-keystroke path.
pub struct FileCache {
    pub paths: Vec<String>,
    pub haystacks: Vec<Utf32String>,
}

/// Shared between the UI thread (which clears it on ctrl-r) and workers
/// (which build it on demand). `None` means "not built yet".
pub type SharedCache = Arc<Mutex<Option<Arc<FileCache>>>>;

pub fn empty_cache() -> SharedCache {
    Arc::new(Mutex::new(None))
}

fn build(walk: WalkOpts) -> FileCache {
    let mut paths = Vec::new();
    // Single-threaded walk: this runs once and is dominated by syscalls, not
    // by matching. Same ignore rules as the grep engine.
    let mut builder = WalkBuilder::new(".");
    walk.apply(&mut builder);
    for entry in builder.build().flatten() {
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let path = entry.path();
        let path = path.strip_prefix("./").unwrap_or(path);
        paths.push(path.to_string_lossy().into_owned());
    }
    let haystacks = paths.iter().cloned().map(Utf32String::from).collect();
    FileCache { paths, haystacks }
}

/// Fuzzy-match `query` against every cached path. Runs to completion on the
/// calling thread; call it from a worker.
/// `limit` caps results — see the note on `engine::search`.
pub fn search(
    query: &str,
    walk: WalkOpts,
    limit: usize,
    generation: u64,
    current_gen: Arc<AtomicU64>,
    cache: SharedCache,
    tx: Sender<SearchMsg>,
) {
    let started = Instant::now();

    // Built under the lock so two quick keystrokes can't both walk the tree.
    let snapshot = {
        let mut guard = cache.lock().expect("file cache mutex poisoned");
        if guard.is_none() {
            *guard = Some(Arc::new(build(walk)));
        }
        Arc::clone(guard.as_ref().expect("just built"))
    };

    // The walk may have taken a while; bail if we're already stale.
    if current_gen.load(Ordering::Relaxed) != generation {
        return;
    }

    // match_paths gives a scoring bonus to characters after a `/`, so typing
    // "mainrs" ranks src/main.rs above src/domain/parser.rs.
    let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
    let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);

    let mut scored: Vec<(u32, usize)> = snapshot
        .haystacks
        .iter()
        .enumerate()
        .filter_map(|(i, h)| pattern.score(h.slice(..), &mut matcher).map(|s| (s, i)))
        .collect();

    // Best score first; ties broken by path so the list doesn't jitter between
    // runs of the same query.
    scored.sort_unstable_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| snapshot.paths[a.1].cmp(&snapshot.paths[b.1]))
    });
    scored.truncate(limit);

    for chunk in scored.chunks(BATCH) {
        if current_gen.load(Ordering::Relaxed) != generation {
            return;
        }
        let hits: Vec<Hit> = chunk
            .iter()
            .map(|&(_, i)| Hit {
                path: snapshot.paths[i].clone(),
                // The opener contract substitutes 1 when there's no position.
                line_number: 1,
                col: 1,
                line_text: String::new(),
            })
            .collect();
        let _ = tx.send(SearchMsg::Hits(generation, hits));
    }

    let _ = tx.send(SearchMsg::Done(generation, started.elapsed().as_millis()));
}
