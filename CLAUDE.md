# tmux-haha

`ta`: find, start and switch tmux sessions. Rust, one binary, no ties to any other tool. Open source under `larrykoo711/tmux-haha`.

## Layout

| Path | Purpose |
|------|---------|
| `src/main.rs` | CLI (`ta <name>`, `ta new`, `ta ls`) |
| `src/tmux.rs` | tmux adapter: every tmux command goes through a `Runner` with a timeout |
| `src/direct.rs` | which session a name means |
| `src/enter.rs` | switch (inside tmux) or attach (outside) |
| `src/naming.rs` | session names: what tmux can take, `-2` for taken names |
| `src/i18n.rs` | English / Chinese, `tr!` |
| `tests/e2e.rs` | end-to-end tests on a private tmux server |

## Conventions

- Everything in this repo is public: no hostnames, private IPs, account names or credentials.
- Never set global tmux options or key bindings, never end the tmux server: it may be shared with other tools.
- End-to-end tests run against a private tmux socket (`-L ta-test-…`) and a temporary `HOME`, never the user's tmux server.
- Before committing, run what CI runs: `cargo fmt --all --check && cargo clippy --all-targets -- -D warnings && cargo test --all`.
- Commits: English Conventional Commits, one subject line plus bullet points.
- The README is Chinese first (`README.md`) with an English twin (`README.en.md`); change both together.
