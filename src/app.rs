use crate::rg::Hit;
use std::collections::BTreeSet;

pub struct App {
    pub query: String,
    /// Byte index into `query`, always on a char boundary.
    pub cursor: usize,
    pub hits: Vec<Hit>,
    pub selected: usize,
    pub marked: BTreeSet<usize>,
    /// Bumped on every query edit; results tagged with an older value are stale.
    pub generation: u64,
    pub status: String,
    pub should_quit: bool,
}

impl App {
    pub fn new(query: String) -> Self {
        Self {
            cursor: query.len(),
            query,
            hits: Vec::new(),
            selected: 0,
            marked: BTreeSet::new(),
            generation: 0,
            status: String::new(),
            should_quit: false,
        }
    }

    pub fn insert(&mut self, c: char) {
        self.query.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    pub fn backspace(&mut self) {
        let Some(prev) = self.query[..self.cursor].chars().next_back() else {
            return;
        };
        self.cursor -= prev.len_utf8();
        self.query.remove(self.cursor);
    }

    pub fn move_left(&mut self) {
        if let Some(prev) = self.query[..self.cursor].chars().next_back() {
            self.cursor -= prev.len_utf8();
        }
    }

    pub fn move_right(&mut self) {
        if let Some(next) = self.query[self.cursor..].chars().next() {
            self.cursor += next.len_utf8();
        }
    }

    /// Column to draw the caret at. Char count, not byte count — close enough
    /// until someone types a CJK character into a query (v2: unicode-width).
    pub fn cursor_col(&self) -> u16 {
        self.query[..self.cursor].chars().count() as u16
    }

    pub fn move_selection(&mut self, delta: isize) {
        if self.hits.is_empty() {
            self.selected = 0;
            return;
        }
        let last = self.hits.len() - 1;
        self.selected = self.selected.saturating_add_signed(delta).min(last);
    }

    pub fn toggle_mark(&mut self) {
        if self.hits.is_empty() {
            return;
        }
        if !self.marked.insert(self.selected) {
            self.marked.remove(&self.selected);
        }
    }

    /// Accept results only if they belong to the current generation.
    pub fn set_hits(&mut self, generation: u64, hits: Vec<Hit>, elapsed_ms: u128) {
        if generation != self.generation {
            return;
        }
        // `+` because the search stops at MAX_HITS — there may be more.
        let more = if hits.len() >= crate::rg::MAX_HITS { "+" } else { "" };
        self.status = format!("{}{more} matches in {elapsed_ms}ms", hits.len());
        self.hits = hits;
        // Marks index into `hits`; a new result set makes them meaningless.
        self.marked.clear();
        self.selected = 0;
    }

    /// A failed search (bad regex, rg missing) — surface it in the status bar
    /// rather than silently showing zero results.
    pub fn set_error(&mut self, generation: u64, message: String) {
        if generation != self.generation {
            return;
        }
        self.hits.clear();
        self.marked.clear();
        self.selected = 0;
        self.status = message;
    }

    /// What `enter` acts on: every marked hit, or the selected one.
    pub fn targets(&self) -> Vec<&Hit> {
        if self.marked.is_empty() {
            self.hits.get(self.selected).into_iter().collect()
        } else {
            self.marked
                .iter()
                .filter_map(|&i| self.hits.get(i))
                .collect()
        }
    }
}
