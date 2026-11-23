# Changelog

All notable changes to tmux-haha (`ta`) are recorded here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). Before 1.0, minor versions may change behaviour.

## [Unreleased]

## [0.1.0] - 2026-11-23

The first release: one small command to find, start and switch tmux sessions.

### Added

- **`ta` opens a session picker.** Type to search, arrows to move, Enter to go (switch inside tmux, attach outside),
  right arrow to rename or close. Closing always asks first, with Keep selected, and the session you are in can't be
  closed from there. Enter on a search with no match starts a session with that name.
- **Windows and folders tabs.** Tab moves between sessions, windows (grouped by session) and folders: the folders
  zoxide knows, then the folders inside the roots you configure. Enter on a folder goes to its session, or starts one
  there.
- **`ta <name>`** goes straight to a session, starting it in the current folder if there is none; **`ta new`** starts
  one named after the folder (`-2`, `-3`… when taken); **`ta ls`** lists sessions; `-d` starts or finds without going
  there.
- **Living with other tools.** `ta` never changes global tmux options or key bindings and never ends the server. A
  session with the `@managed-by` option belongs to that tool: `ta` lists it with the tool's name but only goes to it.
  [cc-moss](https://github.com/larrykoo711/cc-moss) 0.3.16 and later set `@managed-by cc-moss` on their sessions.
- **Configuration** in `~/.config/ta/config.toml`: the tmux server, a host prefix for new session names, the language
  (English or Chinese) and the folder roots. A broken file stops `ta` with the file and the mistake.
- Prebuilt binaries for macOS (Apple silicon, Intel) and Linux (x86_64, arm64, static musl), plus a Homebrew formula.

[Unreleased]: https://github.com/larrykoo711/tmux-haha/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/larrykoo711/tmux-haha/releases/tag/v0.1.0
