# Brewinget — Claude Session Guide

## Project Overview

Brewinget is a native desktop app that puts a clean GUI on top of command-line package managers.
It uses **Tauri v2** (Rust) for the backend and plain **HTML/CSS/JavaScript** for the frontend —
no React, no bundler, no build step for the frontend. The goal is a fast, polished, offline-first
app that feels native on Windows and macOS.

**Target platforms:**
| Platform | Package manager | Backend module |
|----------|----------------|----------------|
| Windows  | `winget`       | `src-tauri/src/winget.rs` |
| macOS    | `brew`         | `src-tauri/src/brew.rs`   |

**Current MVP features:** search packages · install with live log · list installed packages ·
filter/sort installed list · hide ARP registry entries · hide MS Store noise · update individual
packages · update all packages at once · source badges · skeleton loading states.

**Windows elevation:** Brewinget on Windows requires administrator privileges and requests them at
launch via UAC (`requireAdministrator` in `src-tauri/brewinget.manifest`, embedded via `build.rs`).
One UAC prompt at app start — no further prompts during use. macOS is unaffected.
**Dev tip:** run `npm run tauri dev` from an already-elevated PowerShell terminal to avoid a UAC
prompt on every hot-reload cycle.

---

## Architecture

### Frontend (`src/`)
```
src/index.html   — single HTML page, two tabs: Search and Installed
src/main.js      — all JS: tab switching, search, installed list, log modal
src/styles.css   — all CSS: dark theme via CSS vars, cards, table, modal
```
No build step. Tauri serves `src/` directly (see `tauri.conf.json` → `frontendDist: "../src"`).
Tauri globals are available because `withGlobalTauri: true` is set — access them as
`window.__TAURI__.core.invoke` and `window.__TAURI__.event.listen`.

### Backend (`src-tauri/src/`)
```
main.rs   — one-liner entry point, calls brewinget_lib::run()
lib.rs    — Tauri command registration + shared helpers (run_streamed, strip_ansi)
winget.rs — Windows: search, install, update, update-all, list-installed, table parser
brew.rs   — macOS: same public API surface as winget.rs
```

**Conditional compilation** — `lib.rs` picks the right module at compile time:
```rust
#[cfg(target_os = "windows")] mod winget;
#[cfg(target_os = "windows")] use winget as pm;
// same pattern for macos / brew
```
Every public function in `winget.rs` must have a matching signature in `brew.rs`.

### Tauri commands (registered in `lib.rs`)
| Command | Args | Returns |
|---------|------|---------|
| `search_packages` | `query: String` | `Result<Vec<Value>>` |
| `install_package` | `package: String` | void (streams events) |
| `list_installed` | — | `Result<Vec<Value>>` |
| `update_package` | `package: String` | void (streams events) |
| `update_all_packages` | — | void (streams events) |

Streaming commands emit two Tauri events: `install-output` (one line at a time) and
`install-complete` (`"success"` or `"error"`).

### Key design decisions
- winget is called via `cmd /c winget` — direct spawn fails for GUI processes because winget is an
  app execution alias that requires a console host.
- `parse_table` in `winget.rs` handles winget's fixed-width columns and `\r` spinner overwrites by
  calling `rsplit('\r').next()` on each line before parsing.
- ARP entries (`id` starts with `"ARP\\"`) are flagged in `list_installed` so the UI can hide them.
  They appear in `winget list` but cannot be managed by winget.
- MS Store packages are identified by a pure uppercase-alphanumeric ID 9–13 chars long, or
  `source == "msstore"`. They clutter search results so a toggle hides them by default.

---

## Development Workflow

```bash
# Install JS deps (first time only)
npm install

# Run dev server — hot-reloads the frontend, recompiles Rust on change
npm run tauri dev

# Production build — output in src-tauri/target/release/bundle/
npm run tauri build
```

The dev window opens at whatever Tauri decides (no fixed port — it's a native window, not a
browser tab). Frontend changes reflect immediately; Rust changes trigger a recompile (~5–30 s).

**Git workflow Jacob prefers:** commit after each self-contained feature, push frequently, test
in the running app before pushing.

---

## Conventions

**JavaScript** — vanilla ES modules (`type="module"` on the script tag). No frameworks, no npm
packages in the frontend. All DOM refs are captured at the top of `main.js`.

**CSS** — all design tokens live in `:root` as CSS custom properties. Color names: `--bg`,
`--surface`, `--card`, `--border`, `--accent`, `--muted`, `--text`, `--success`, `--warning`,
`--error`. Add new tokens there rather than hardcoding hex values.

**Rust** — `snake_case` for functions and variables; command names in Tauri use `snake_case` and
map to JS `invoke('snake_case_name')`. Keep `winget.rs` and `brew.rs` in sync — same public
function names and signatures.

**Error handling** — Rust commands return `Result<T, String>`; the JS side shows the error string
in the status line or log footer. Never `unwrap()` in command handlers. Streaming commands use
events rather than return values, so errors are emitted as `install-output` + `install-complete`.

**Comments** — only write one when the *why* is non-obvious. The winget/cmd quirk, the ARP
pattern, the `\r` spinner trick — those deserve comments. Obvious things don't.

---

## Known Gotchas

**winget table parsing is fragile.** The column widths are fixed per-run but vary by content and
terminal width. The parser reads column offsets from the header line on each invocation — don't
assume fixed byte positions.

**winget `\r` spinner.** When stdout is piped, winget's progress spinner writes multiple frames
on the same line separated by `\r`. `rsplit('\r').next()` grabs the last (real) content.

**BOM on first line.** winget sometimes emits a UTF-8 BOM (`\u{feff}`) on the first line. The
parser strips it explicitly.

**ARP entries.** `winget list` returns everything in the Add/Remove Programs registry, including
entries winget cannot manage. These have IDs starting with `ARP\`. Always filter them unless the
user explicitly opts in.

**MS Store IDs.** Pure uppercase-alphanumeric strings (e.g. `9NBLGGH4NNS1`). They appear in
search results but install/update behavior is inconsistent — the user can hide them via toggle.

**`cargo clean` after rename.** When the project was renamed from `winget-gui` to `brewinget`,
the Tauri build script had cached the old path in `target/`. `cargo clean` fixed it. If you see a
build error referencing an old path, clean first.

**`withGlobalTauri: true` is required.** Without it, `window.__TAURI__` is undefined and every
`invoke` call silently fails.

---

## User Preferences

- Jacob is learning — explain things in plain English, not jargon-first.
- Prefer system fonts (`-apple-system`, `Segoe UI`, etc.) — no Google Fonts or CDN imports.
- Work in steps: make a change, describe what it does and why, then move on.
- Always verify Rust compiles (`cargo check`) before reporting a backend change as done.
- Test the running app manually before pushing to GitHub.
- Commit style: one commit per logical feature, present-tense subject line, no issue references.
