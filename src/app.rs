use crate::preview::{self, Preview};
use crate::search::{Hit, MAX_HITS};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Query is a regex, matched against file contents.
    Grep,
    /// Query fuzzy-matches file paths.
    Files,
}

impl Mode {
    pub fn label(self) -> &'static str {
        match self {
            Mode::Grep => " GREP ",
            Mode::Files => " FILES ",
        }
    }
}

pub struct App {
    pub query: String,
    /// Byte index into `query`, always on a char boundary.
    pub cursor: usize,
    pub hits: Vec<Hit>,
    pub selected: usize,
    pub marked: BTreeSet<usize>,
    pub mode: Mode,
    /// Bumped on every query edit; results tagged with an older value are stale.
    pub generation: u64,
    /// Generation currently on screen. Results are cleared when the first
    /// batch of a *new* generation lands, not on keystroke — otherwise the
    /// list blinks empty every time you type.
    displayed_generation: u64,
    pub status: String,
    pub should_quit: bool,
    /// Set by `-r`. Applied once the resumed search completes, since the hits
    /// it indexes into don't exist yet at startup.
    pub restore_selected: Option<usize>,
    pub show_preview: bool,
    /// Cached so we don't re-read the file on every 30ms redraw.
    preview: Option<Preview>,
}

impl App {
    pub fn new(query: String, mode: Mode) -> Self {
        Self {
            cursor: query.len(),
            query,
            hits: Vec::new(),
            selected: 0,
            marked: BTreeSet::new(),
            mode,
            generation: 0,
            displayed_generation: 0,
            status: String::new(),
            should_quit: false,
            restore_selected: None,
            show_preview: false,
            preview: None,
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

    pub fn preview(&self) -> Option<&Preview> {
        self.preview.as_ref()
    }

    /// Reload the preview only when the selected hit actually changed. Called
    /// once per loop iteration; the common case is a cheap comparison.
    pub fn refresh_preview(&mut self) {
        if !self.show_preview {
            self.preview = None;
            return;
        }
        let Some(hit) = self.hits.get(self.selected) else {
            self.preview = None;
            return;
        };
        let stale = match &self.preview {
            Some(p) => p.path != hit.path || p.line != hit.line_number,
            None => true,
        };
        if stale {
            self.preview = preview::load(&hit.path, hit.line_number);
        }
    }

    pub fn toggle_mode(&mut self) {
        self.mode = match self.mode {
            Mode::Grep => Mode::Files,
            Mode::Files => Mode::Grep,
        };
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
    /// Drop everything on screen if these results belong to a newer search.
    fn adopt(&mut self, generation: u64) {
        if self.displayed_generation != generation {
            self.hits.clear();
            // Marks index into `hits`; a new result set makes them meaningless.
            self.marked.clear();
            self.selected = 0;
            self.displayed_generation = generation;
        }
    }

    /// A streamed batch. Many of these arrive per search.
    pub fn append_hits(&mut self, generation: u64, hits: Vec<Hit>) {
        if generation != self.generation {
            return;
        }
        self.adopt(generation);
        self.hits.extend(hits);
    }

    /// The walk finished. Also handles the zero-result case, where no batch
    /// ever arrived to trigger `adopt`.
    pub fn finish(&mut self, generation: u64, elapsed_ms: u128) {
        if generation != self.generation {
            return;
        }
        self.adopt(generation);
        if let Some(want) = self.restore_selected.take() {
            self.selected = want.min(self.hits.len().saturating_sub(1));
        }
        // `+` because the search stops at MAX_HITS — there may be more.
        let more = if self.hits.len() >= MAX_HITS { "+" } else { "" };
        self.status = format!("{}{more} matches in {elapsed_ms}ms", self.hits.len());
    }

    /// A failed search (bad regex, bad type filter) — surface it in the status
    /// bar rather than silently showing zero results.
    pub fn set_error(&mut self, generation: u64, message: String) {
        if generation != self.generation {
            return;
        }
        self.adopt(generation);
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
