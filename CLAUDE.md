# tmux-haha

`ta`: find, start and switch tmux sessions. Rust, one binary, no ties to any other tool. Open source under `larrykoo711/tmux-haha`.

## Layout

| Path | Purpose |
|------|---------|
| `src/main.rs` | CLI (`ta`, `ta <name>`, `ta new`, `ta ls`) |
| `src/pick/` | the picker: `app.rs` state and keys, `ui.rs` drawing, `mod.rs` terminal and event loop |
| `src/config.rs` | `~/.config/ta/config.toml`: defaults when missing, an error naming the file when broken |
| `src/folders.rs` | the folder tab's folders: zoxide (1 s at most), then the configured roots |
| `src/tmux.rs` | tmux adapter: every tmux command goes through a `Runner` with a timeout |
| `src/direct.rs` | which session a name means |
| `src/enter.rs` | switch (inside tmux) or attach (outside) |
| `src/naming.rs` | session names: what tmux can take, `-2` for taken names |
| `src/i18n.rs` | English / Chinese, `tr!` |
| `tests/e2e.rs` | end-to-end tests on a private tmux server |
| `packaging/homebrew/` | Homebrew formula template and `render.sh`; `release.yml` renders it for `larrykoo711/homebrew-tap` |
| `tests/picker.rs` | the picker in a real pane on a private server, driven by `tmux send-keys` |

## Conventions

- Everything in this repo is public: no hostnames, private IPs, account names or credentials.
- tmux commands that change a session target its id (`$N`), never its name.
- A session with `@managed-by` set belongs to another tool: only go to it, never rename or close it.
- Never set global tmux options or key bindings, never end the tmux server: it may be shared with other tools.
- End-to-end tests run against a private tmux socket (`-L ta-test-…`) and a temporary `HOME`, never the user's tmux server.
- Before committing, run what CI runs: `cargo fmt --all --check && cargo clippy --all-targets -- -D warnings && cargo test --all`.
- Commits: English Conventional Commits, one subject line plus bullet points.
- The README is Chinese first (`README.md`) with an English twin (`README.en.md`); change both together.
