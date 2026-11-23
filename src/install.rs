//! `ta install`: shell completions, and (only if you agree) a tmux key that
//! opens ta in a popup. Every change is listed and asked for first;
//! `--uninstall` takes it all out again.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::CommandFactory;

use crate::block;
use crate::cli::{Cli, ShellName};
use crate::tmux::{ProcessRunner, Tmux};

const COMPLETIONS: &str = "ta: completions";
const POPUP: &str = "ta: popup key";
/// The first line of the fish file ta writes, so it only ever removes its own.
const FISH_HEADER: &str =
    "# ta completions (written by `ta install`; `ta install --uninstall` removes it)";
const POPUP_KEY: &str = "bind-key T display-popup -E -w 90% -h 85% \"ta\"";

/// The completion script for `shell`.
pub fn completions(shell: ShellName) -> String {
    let mut out = Vec::new();
    clap_complete::generate(shell.generator(), &mut Cli::command(), "ta", &mut out);
    String::from_utf8_lossy(&out).into_owned()
}

/// The user's shell, from `$SHELL`.
pub fn detect_shell() -> Option<ShellName> {
    let shell = std::env::var("SHELL").ok()?;
    match Path::new(&shell).file_name()?.to_str()? {
        "zsh" => Some(ShellName::Zsh),
        "bash" => Some(ShellName::Bash),
        "fish" => Some(ShellName::Fish),
        _ => None,
    }
}

fn home() -> Result<PathBuf> {
    std::env::var("HOME")
        .ok()
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
        .context("HOME is not set")
}

/// The rc file or completion file for `shell`.
fn shell_file(shell: ShellName, home: &Path) -> PathBuf {
    match shell {
        ShellName::Zsh => home.join(".zshrc"),
        ShellName::Bash => home.join(".bashrc"),
        ShellName::Fish => home.join(".config/fish/completions/ta.fish"),
    }
}

/// The tmux config: `~/.tmux.conf`, else `~/.config/tmux/tmux.conf` if that exists.
fn tmux_conf(home: &Path) -> PathBuf {
    let classic = home.join(".tmux.conf");
    let xdg = home.join(".config/tmux/tmux.conf");
    if !classic.exists() && xdg.exists() {
        xdg
    } else {
        classic
    }
}

/// Whether the shell's completions are set up (for `ta doctor`).
pub fn completions_installed(shell: ShellName) -> bool {
    let Ok(home) = home() else { return false };
    let path = shell_file(shell, &home);
    let text = std::fs::read_to_string(real(&path)).unwrap_or_default();
    match shell {
        ShellName::Fish => text.starts_with(FISH_HEADER),
        _ => block::contains(&text, COMPLETIONS),
    }
}

/// One file ta would change.
struct Change {
    path: PathBuf,
    what: String,
    /// The file's new text; `None` deletes it.
    after: Option<String>,
}

/// Notes ta keeps inside its own block, so taking it out puts the file
/// back exactly as it was.
const NOTE_CREATED: &str = "# (ta created this file)";
const NOTE_NEWLINE: &str = "# (ta ended the line above with a newline)";

/// The file a path names: a link's target, so a dotfiles link stays a link.
fn real(path: &Path) -> PathBuf {
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => {
            std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
        }
        _ => path.to_path_buf(),
    }
}

/// The file's text; `None` when there is no file. Anything ta can't read
/// as text is an error, never an empty file to write over.
fn read(path: &Path) -> Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => bail!(tr!(
            format!(
                "cannot read {} ({e}), so ta leaves it alone",
                path.display()
            ),
            format!("读不了 {}（{e}），ta 不会改它", path.display())
        )),
    }
}

/// `before` with ta's block holding `body`, noting what ta did to the file
/// around it. A block already there keeps its notes.
fn with_block(before: Option<&str>, tag: &str, body: &str) -> Result<String> {
    let text = before.unwrap_or("");
    let notes: Vec<&str> = match block::parts(text, tag)? {
        Some((_, old, _)) => old
            .lines()
            .filter(|l| *l == NOTE_CREATED || *l == NOTE_NEWLINE)
            .map(|l| {
                if l == NOTE_CREATED {
                    NOTE_CREATED
                } else {
                    NOTE_NEWLINE
                }
            })
            .collect(),
        None => {
            let mut notes = Vec::new();
            if before.is_none() {
                notes.push(NOTE_CREATED);
            }
            if !text.is_empty() && !text.ends_with('\n') {
                notes.push(NOTE_NEWLINE);
            }
            notes
        }
    };
    let mut full = notes.join("\n");
    if !full.is_empty() {
        full.push('\n');
    }
    full.push_str(body);
    block::upsert(text, tag, &full)
}

/// `text` without ta's block, as it was before ta added it; `None` when the
/// file was ta's own and should go.
fn without_block(text: &str, tag: &str) -> Result<Option<String>> {
    let Some((mut before, body, after)) = block::parts(text, tag)? else {
        return Ok(Some(text.to_string()));
    };
    if after.is_empty() && body.lines().any(|l| l == NOTE_NEWLINE) {
        if before.ends_with("\r\n") {
            before.truncate(before.len() - 2);
        } else if before.ends_with('\n') {
            before.pop();
        }
    }
    let rest = format!("{before}{after}");
    Ok(
        if rest.is_empty() && body.lines().any(|l| l == NOTE_CREATED) {
            None
        } else {
            Some(rest)
        },
    )
}

/// The line that loads the completions. zsh needs `compdef` (compinit) first;
/// `eval` also works in macOS's bash 3.2, where `source <(…)` does not.
fn completion_line(shell: ShellName) -> String {
    match shell {
        ShellName::Zsh => "if command -v ta >/dev/null 2>&1 && (( $+functions[compdef] )); then eval \"$(ta completions zsh)\"; fi".to_string(),
        _ => format!("if command -v ta >/dev/null 2>&1; then eval \"$(ta completions {shell})\"; fi"),
    }
}

fn completion_change(shell: ShellName, home: &Path, uninstall: bool) -> Result<Option<Change>> {
    let path = real(&shell_file(shell, home));
    let before = read(&path)?;
    let what = tr!(
        format!("{shell} completions for ta"),
        format!("ta 的 {shell} 补全")
    );
    let after = match (shell, uninstall) {
        (ShellName::Fish, false) => match &before {
            Some(text) if !text.starts_with(FISH_HEADER) => {
                println!(
                    "{}",
                    tr!(
                        format!(
                            "{} is your own file, so ta leaves it alone.",
                            path.display()
                        ),
                        format!("{} 是你自己的文件，ta 不动它。", path.display())
                    )
                );
                return Ok(None);
            }
            _ => Some(format!("{FISH_HEADER}\n{}", completions(shell))),
        },
        (ShellName::Fish, true) => match &before {
            Some(text) if text.starts_with(FISH_HEADER) => None,
            _ => return Ok(None),
        },
        (_, false) => Some(with_block(
            before.as_deref(),
            COMPLETIONS,
            &completion_line(shell),
        )?),
        (_, true) => match &before {
            Some(text) => without_block(text, COMPLETIONS)?,
            None => return Ok(None),
        },
    };
    let unchanged = match (&after, &before) {
        (Some(new), Some(old)) => new == old,
        (None, None) => true,
        _ => false,
    };
    Ok((!unchanged).then_some(Change { path, what, after }))
}

fn popup_change(home: &Path, uninstall: bool) -> Result<Option<Change>> {
    let path = real(&tmux_conf(home));
    let before = read(&path)?;
    let after = match (uninstall, &before) {
        (true, None) => return Ok(None),
        (true, Some(text)) => without_block(text, POPUP)?,
        (false, _) => Some(with_block(before.as_deref(), POPUP, POPUP_KEY)?),
    };
    let unchanged = match (&after, &before) {
        (Some(new), Some(old)) => new == old,
        (None, None) => true,
        _ => false,
    };
    Ok((!unchanged).then(|| Change {
        path,
        what: tr!(
            "a tmux key, prefix + T, that opens ta in a popup".to_string(),
            "一个 tmux 按键：prefix + T 用弹窗打开 ta".to_string()
        ),
        after,
    }))
}

/// Whether this tmux has `display-popup` (3.2 and later).
fn has_popup(tmux: &Tmux<ProcessRunner>) -> bool {
    tmux.version().is_some_and(|v| supports_popup(&v))
}

/// `tmux 3.3a`, `tmux next-3.6`… at least 3.2.
fn supports_popup(version: &str) -> bool {
    let digits: String = version
        .trim_start_matches("tmux ")
        .trim_start_matches("next-")
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let mut parts = digits.split('.').filter_map(|p| p.parse::<u32>().ok());
    (parts.next().unwrap_or(0), parts.next().unwrap_or(0)) >= (3, 2)
}

fn write(change: &Change) -> Result<()> {
    match &change.after {
        None => std::fs::remove_file(&change.path)
            .with_context(|| format!("cannot remove {}", change.path.display())),
        Some(text) => {
            if let Some(dir) = change.path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            // Write next to it, then rename: never a half-written rc file.
            let tmp = change
                .path
                .with_extension(format!("ta-{}", std::process::id()));
            std::fs::write(&tmp, text)?;
            if let Ok(meta) = std::fs::metadata(&change.path) {
                std::fs::set_permissions(&tmp, meta.permissions())?;
            }
            std::fs::rename(&tmp, &change.path)
                .with_context(|| format!("cannot write {}", change.path.display()))
        }
    }
}

pub struct Options {
    pub yes: bool,
    pub uninstall: bool,
    pub shell: Option<ShellName>,
    pub no_tmux_key: bool,
}

pub fn run(tmux: &Tmux<ProcessRunner>, opts: Options) -> Result<()> {
    let home = home()?;
    let shell = opts.shell.or_else(detect_shell);
    let mut changes = Vec::new();
    if opts.uninstall {
        for shell in [ShellName::Zsh, ShellName::Bash, ShellName::Fish] {
            changes.extend(completion_change(shell, &home, true)?);
        }
        changes.extend(popup_change(&home, true)?);
    } else {
        match shell {
            Some(shell) => changes.extend(completion_change(shell, &home, false)?),
            None => println!(
                "{}",
                tr!(
                    "Can't tell your shell from $SHELL; pick one with --shell zsh|bash|fish.",
                    "从 $SHELL 看不出你用哪个 shell；用 --shell zsh|bash|fish 指定。"
                )
            ),
        }
        if !opts.no_tmux_key {
            if has_popup(tmux) {
                changes.extend(popup_change(&home, false)?);
            } else {
                println!(
                    "{}",
                    tr!(
                        "Skipping the tmux popup key: it needs tmux 3.2 or later.",
                        "跳过 tmux 弹窗按键：需要 tmux 3.2 或更新的版本。"
                    )
                );
            }
        }
    }
    if changes.is_empty() {
        println!("{}", tr!("Nothing to change.", "没有需要改的。"));
        return Ok(());
    }
    let home_text = home.to_string_lossy().into_owned();
    let mut done = 0;
    for change in &changes {
        let path = crate::doctor::tilde(&change.path.to_string_lossy(), &home_text);
        let question = match (&change.after, opts.uninstall) {
            (None, _) | (Some(_), true) => tr!(
                format!("Remove {} from {path}?", change.what),
                format!("从 {path} 移除 {}？", change.what)
            ),
            (Some(_), false) => tr!(
                format!("Add {} to {path}?", change.what),
                format!("把 {} 加到 {path}？", change.what)
            ),
        };
        let refused = tr!(
            "add --yes to go ahead without asking",
            "加上 --yes 就不用问了"
        );
        if crate::confirm::ask(opts.yes, &question, refused, true)? {
            write(change)?;
            println!("  ✓ {path}");
            done += 1;
        }
    }
    if done > 0 && !opts.uninstall {
        println!(
            "{}",
            tr!(
                "Open a new shell for the completions; for the tmux key, run `tmux source-file` on your tmux config (or restart tmux).",
                "补全在新开的 shell 里生效；tmux 按键要对 tmux 配置运行一次 `tmux source-file`（或重启 tmux）。"
            )
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn popup_needs_tmux_3_2() {
        for yes in ["tmux 3.2", "tmux 3.3a", "tmux 3.10", "tmux next-3.6"] {
            assert!(supports_popup(yes), "{yes}");
        }
        for no in ["tmux 3.1c", "tmux 2.9", "garbage"] {
            assert!(!supports_popup(no), "{no}");
        }
    }
}
