# ripf

Interactive code search for the terminal. Type a query, watch matches appear live, hit enter — your editor opens at the exact line and column.

ripgrep's engine underneath, fuzzy finding on top, and a configurable open command so it works with whatever editor you already use.

```
$ ripf "open_command"
```

## Why not just `rg | fzf`

`rg | fzf` searches once and filters the output. `ripf` re-runs the search as you type, so you're refining the *query*, not sifting through a frozen result set.

Beyond that:

- **Exact position opening.** Results carry `file:line:col`, and your editor lands on the match — not the top of the file.
- **Two modes, one binary.** `ctrl-f` flips between content search and fuzzy filename search. No second tool, no second pipeline.
- **Multi-select.** Mark several results, open them all at once.
- **Preview.** See the surrounding code before you commit to opening it.
- **Resume.** `ripf -r` picks up your last query, mode, and cursor position.
- **One config file.** One `open_command` string is the entire editor integration.

## Install

```bash
cargo install ripf
```

ripgrep's search engine is compiled in, so there's nothing else to install. (Through 0.2, `ripf` shells out to the `rg` binary instead and needs it on your `PATH` — see [Status](#status).)

Then point it at your editor:

```bash
mkdir -p ~/.config/ripf
echo 'open_command = "cursor -g {file}:{line}:{col}"' > ~/.config/ripf/config.toml
```

## Usage

```bash
ripf "document"          # live grep across the repo, pick a result, open it
ripf                     # open the TUI with an empty query
ripf -t rust "transfer"  # restrict to Rust files
ripf -f                  # start in filename fuzzy mode
ripf -r                  # resume last session (query + mode + cursor)
ripf --print "foo"       # non-interactive: file:line:col to stdout, for piping
```

`--print` follows grep's exit-code convention: `0` when there were matches, `1` when there were none, `2` on error. That makes it safe in scripts and pipelines:

```bash
ripf --print "TODO" | wc -l
ripf --print "unwrap()" -t rust || echo "clean"
```

### Flags

| Flag | Effect |
|---|---|
| `-t`, `--type <TYPE>` | Restrict to a file type (`rust`, `toml`, `py`, …). Repeatable. |
| `-f`, `--files` | Start in filename fuzzy mode instead of content mode. |
| `-r`, `--resume` | Restore the last session's query, mode, and cursor. |
| `--print` | Print `file:line:col` to stdout and exit. No TUI. |
| `--open <CMD>` | Override `open_command` for this run. |
| `--hidden` | Include hidden files. |
| `--no-ignore` | Ignore `.gitignore` and friends. |

## Keys

| Key | Action |
|---|---|
| `↑` / `↓`, `ctrl-k` / `ctrl-j` | Move selection |
| `enter` | Open selection (or every marked result) |
| `tab` | Mark / unmark a result |
| `ctrl-f` | Toggle GREP ↔ FILES mode |
| `ctrl-p` | Toggle the preview pane |
| `ctrl-r` | Rebuild the file cache (FILES mode) |
| `esc`, `ctrl-c` | Quit |

## Configuration

`~/.config/ripf/config.toml` (or `$XDG_CONFIG_HOME/ripf/config.toml`):

```toml
# Required. {file}, {line} and {col} are substituted per result.
open_command = "cursor -g {file}:{line}:{col}"

# Exit after opening a result. Default: true.
quit_on_open = true

# Optional key overrides.
[keys]
toggle_mode = "ctrl-f"
toggle_preview = "ctrl-p"
```

If `open_command` is unset, `ripf` falls back to `$EDITOR`. Precedence is `--open` → config file → `$EDITOR` → error.

### Editor commands

| Editor | `open_command` |
|---|---|
| Cursor | `cursor -g {file}:{line}:{col}` |
| VS Code | `code -g {file}:{line}:{col}` |
| Zed | `zed {file}:{line}:{col}` |
| Neovim | `nvim +{line} {file}` |
| Helix | `hx {file}:{line}:{col}` |
| Emacs | `emacsclient +{line}:{col} {file}` |
| Sublime Text | `subl {file}:{line}:{col}` |

The command is split with POSIX shell-word rules and executed directly — **no shell is involved**, so filenames containing spaces, quotes or `;` are passed through safely rather than reinterpreted.

`{line}` and `{col}` substitute as `1` in filename mode. A template that omits them is fine.

## Modes

**GREP** — your query is a regex, evaluated by ripgrep's engine across the repo. Smart-case: lowercase queries match case-insensitively, any uppercase character makes the search case-sensitive.

**FILES** — your query fuzzy-matches against file paths, ranked by score, the way `fzf` does.

`.gitignore`, `.ignore` and global git excludes are respected in both modes. `--no-ignore` opts out.

## Status

`ripf` is built in phases. This README describes the finished tool; here's what's actually released:

| | Feature | Status |
|---|---|---|
| 0.1 | `--print`, result list, open at `file:line:col`, `-t` filters | ✅ shipped |
| 0.2 | Live TUI, incremental search, multi-select | 🚧 in progress |
| 0.3 | Native ripgrep engine (no `rg` subprocess) | ⬜ planned |
| 0.4 | Filename fuzzy mode (`-f`, `ctrl-f`) | ⬜ planned |
| 0.5 | Preview pane, resume, key remapping | ⬜ planned |

**Before 0.3, `ripf` shells out to the `rg` binary and requires [ripgrep](https://github.com/BurntSushi/ripgrep) on your `PATH`** (`brew install ripgrep`). From 0.3 onward the engine is compiled in and there is no external dependency.

## Non-goals

- **Search and replace.** Use [serpl](https://github.com/yassinebridi/serpl) or [sad](https://github.com/ms-jpq/sad).
- **Editor plugins.** `ripf` knows how to run a command string. That's the whole integration surface, deliberately.
- **Being a general fuzzy finder.** It searches code and opens files. `fzf` is better at everything else.

## License

MIT OR Apache-2.0, at your option.
