# Changelog

All notable changes to tmux-haha (`ta`) are recorded here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). Before 1.0, minor versions may change behaviour.

## [Unreleased]

### Added

- **Help in English or Chinese.** `ta --help`, `ta help <command>` and usage mistakes follow your language
  (`TA_LANG`, the config's `language`, or the system's).
- **`ta version`** (and `-V`): the name, version, one line and the address in a terminal; `ta <version>` when piped.
- **`ta close <name>`** (asks first; `--yes` doesn't) and **`ta rename <name> <new>`**, by the picker's rules: a session
  another tool manages, or the one you are in, is refused. **`ta ls --json`** and **`ta new --dir <folder>`** for scripts.
- **`ta doctor`** checks tmux, the server, the config, zoxide, the folder roots and completions; **`ta config`** shows
  each setting's value and where it comes from. Both work with a broken config.
- **`ta update`** installs a new version the way you installed ta: `brew upgrade`, `cargo install`, or a download it
  checks against its published checksum before replacing itself. `--check` only looks. Online only when you run it.
- **`ta install`** sets up shell completions (zsh, bash, fish) and, if you agree, a tmux key `prefix + T` that opens
  `ta` in a popup; every change is asked for first, and `--uninstall` takes it all out. **`ta completions <shell>`**
  prints the script.

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
