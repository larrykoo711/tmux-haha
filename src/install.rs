//! `ta install`: shell completions, and (only if you agree) a tmux key that
//! opens ta in a popup. Every change is listed and asked for first;
//! `--uninstall` takes it all out again.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
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
    let text = std::fs::read_to_string(&path).unwrap_or_default();
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

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

fn completion_change(shell: ShellName, home: &Path, uninstall: bool) -> Result<Option<Change>> {
    let path = shell_file(shell, home);
    let before = read(&path);
    let what = tr!(
        format!("{shell} completions for ta"),
        format!("ta 的 {shell} 补全")
    );
    let after = match (shell, uninstall) {
        (ShellName::Fish, false) => Some(format!("{FISH_HEADER}\n{}", completions(shell))),
        (ShellName::Fish, true) if before.starts_with(FISH_HEADER) => None,
        (ShellName::Fish, true) => return Ok(None),
        (_, false) => Some(block::upsert(
            &before,
            COMPLETIONS,
            &format!("if command -v ta >/dev/null 2>&1; then source <(ta completions {shell}); fi"),
        )?),
        (_, true) => Some(block::remove(&before, COMPLETIONS)?),
    };
    let unchanged = match &after {
        Some(text) => *text == before,
        None => !path.exists(),
    };
    Ok((!unchanged).then_some(Change { path, what, after }))
}

fn popup_change(home: &Path, uninstall: bool) -> Result<Option<Change>> {
    let path = tmux_conf(home);
    let before = read(&path);
    let after = if uninstall {
        block::remove(&before, POPUP)?
    } else {
        block::upsert(&before, POPUP, POPUP_KEY)?
    };
    Ok((after != before).then(|| Change {
        path,
        what: tr!(
            "a tmux key, prefix + T, that opens ta in a popup".to_string(),
            "一个 tmux 按键：prefix + T 用弹窗打开 ta".to_string()
        ),
        after: Some(after),
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
