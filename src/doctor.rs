//! `ta doctor`: whether ta can do its job here, one line per thing.

use std::process::Command;
use std::time::Duration;

use anyhow::Result;
use unicode_width::UnicodeWidthStr;

use crate::command::run_with_timeout;
use crate::config::Config;
use crate::tmux::{Access, ProcessRunner, Tmux};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Ok,
    Note,
    Bad,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub level: Level,
    pub what: String,
    pub detail: String,
}

fn check(level: Level, what: &str, detail: String) -> Check {
    Check {
        level,
        what: what.to_string(),
        detail,
    }
}

/// Run every check and print them; false when something is wrong.
pub fn run(tmux: &Tmux<ProcessRunner>, config: &Result<Config>) -> Result<bool> {
    let home = std::env::var("HOME").unwrap_or_default();
    let mut checks = vec![tmux_check(tmux)];
    let tmux_ok = checks[0].level == Level::Ok;
    if tmux_ok {
        checks.push(server_check(tmux));
    }
    checks.push(config_check(config, &home));
    let zoxide = zoxide_version();
    checks.extend(folder_checks(config, zoxide.as_deref(), &home));
    if tmux_ok && cfg!(target_os = "macos") {
        if let Ok(cwd) = std::env::current_dir() {
            let cwd = cwd.to_string_lossy().into_owned();
            if tmux.probe_access(&cwd) == Access::Denied {
                checks.push(check(
                    Level::Bad,
                    tr!("folder access", "目录权限"),
                    tr!(
                        format!("the tmux server cannot read {cwd} (macOS privacy settings); restarting tmux fixes it, but ends every session on it"),
                        format!("tmux server 读不了 {cwd}（macOS 隐私权限）；重启 tmux 能修好，但会结束上面的全部会话")
                    ),
                ));
            }
        }
    }
    print!("{}", render(&checks));
    let bad = checks.iter().filter(|c| c.level == Level::Bad).count();
    println!(
        "\n{}",
        if bad == 0 {
            tr!("ta is ready.".to_string(), "ta 一切就绪。".to_string())
        } else {
            tr!(
                format!("{bad} thing(s) to fix above."),
                format!("上面有 {bad} 项需要处理。")
            )
        }
    );
    Ok(bad == 0)
}

fn tmux_check(tmux: &Tmux<ProcessRunner>) -> Check {
    match tmux.version() {
        Some(v) => check(Level::Ok, "tmux", v),
        None => check(
            Level::Bad,
            "tmux",
            tr!(
                "not installed: `brew install tmux` (macOS) or `sudo apt install tmux` (Debian, Ubuntu)",
                "没装：`brew install tmux`（macOS）或 `sudo apt install tmux`（Debian、Ubuntu）"
            )
            .to_string(),
        ),
    }
}

fn server_check(tmux: &Tmux<ProcessRunner>) -> Check {
    let sessions = tmux.sessions();
    let managed = sessions.iter().filter(|s| s.managed_by.is_some()).count();
    let detail = match (sessions.len(), managed) {
        (0, _) => tr!(
            "no sessions yet (the server starts with the first one)".to_string(),
            "还没有会话（新建第一个时 server 会启动）".to_string()
        ),
        (n, 0) => tr!(format!("{n} session(s)"), format!("{n} 个会话")),
        (n, m) => tr!(
            format!("{n} session(s), {m} managed by other tools (ta only goes to those)"),
            format!("{n} 个会话，其中 {m} 个由别的工具管理（ta 只进入、不改名不关闭）")
        ),
    };
    check(Level::Ok, tr!("tmux server", "tmux server"), detail)
}

fn config_check(config: &Result<Config>, home: &str) -> Check {
    let path = crate::config::path(&|k| std::env::var(k).ok())
        .map(|p| tilde(&p.to_string_lossy(), home))
        .unwrap_or_default();
    let what = tr!("config", "配置");
    match config {
        Err(e) => check(Level::Bad, what, format!("{e:#}")),
        Ok(_) if !std::path::Path::new(&untilde(&path, home)).exists() => check(
            Level::Ok,
            what,
            tr!(
                format!("no {path}; defaults apply"),
                format!("没有 {path}，用默认值")
            ),
        ),
        Ok(_) => check(Level::Ok, what, path),
    }
}

fn folder_checks(config: &Result<Config>, zoxide: Option<&str>, home: &str) -> Vec<Check> {
    let roots = config.as_ref().map(|c| c.roots(home)).unwrap_or_default();
    let mut checks = vec![match zoxide {
        Some(v) => check(Level::Ok, "zoxide", v.to_string()),
        None => check(
            Level::Note,
            "zoxide",
            tr!(
                "not installed: the folders tab lists only the configured roots",
                "没装：目录页只列配置里的根目录"
            )
            .to_string(),
        ),
    }];
    for root in &roots {
        let shown = tilde(root, home);
        checks.push(if std::path::Path::new(root).is_dir() {
            check(Level::Ok, tr!("folder root", "目录根"), shown)
        } else {
            check(
                Level::Note,
                tr!("folder root", "目录根"),
                tr!(format!("{shown} is not there"), format!("{shown} 不存在")),
            )
        });
    }
    if roots.is_empty() && zoxide.is_none() {
        checks.push(check(
            Level::Note,
            tr!("folders tab", "目录页"),
            tr!(
                "empty: install zoxide, or add [folders] roots to the config",
                "会是空的：装上 zoxide，或在配置里加 [folders] roots"
            )
            .to_string(),
        ));
    }
    checks
}

fn zoxide_version() -> Option<String> {
    let out = run_with_timeout(
        Command::new("zoxide").arg("--version"),
        Duration::from_secs(2),
    )
    .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|v| !v.is_empty())
}

/// One line per check, the labels in a column.
pub fn render(checks: &[Check]) -> String {
    let width = checks.iter().map(|c| c.what.width()).max().unwrap_or(0);
    checks
        .iter()
        .map(|c| {
            let mark = match c.level {
                Level::Ok => "✓",
                Level::Note => "!",
                Level::Bad => "✗",
            };
            let pad = " ".repeat(width - c.what.width());
            // A long message (a TOML error) keeps to the detail column.
            let indent = format!("\n{}", " ".repeat(width + 6));
            format!(
                "  {mark} {}{pad}  {}\n",
                c.what,
                c.detail.replace('\n', &indent)
            )
        })
        .collect()
}

pub fn tilde(path: &str, home: &str) -> String {
    match path.strip_prefix(home) {
        Some(rest) if !home.is_empty() && (rest.is_empty() || rest.starts_with('/')) => {
            format!("~{rest}")
        }
        _ => path.to_string(),
    }
}

fn untilde(path: &str, home: &str) -> String {
    match path.strip_prefix('~') {
        Some(rest) => format!("{home}{rest}"),
        None => path.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_line_up() {
        let out = render(&[
            check(Level::Ok, "tmux", "tmux 3.6".into()),
            check(Level::Bad, "配置", "broken".into()),
            check(Level::Note, "zoxide", "missing".into()),
        ]);
        assert_eq!(
            out,
            "  ✓ tmux    tmux 3.6\n  ✗ 配置    broken\n  ! zoxide  missing\n"
        );
    }

    #[test]
    fn long_details_keep_to_their_column() {
        let out = render(&[check(Level::Bad, "config", "line one\nline two".into())]);
        assert_eq!(out, "  ✗ config  line one\n            line two\n");
    }

    #[test]
    fn home_is_shortened() {
        assert_eq!(tilde("/h/me/x", "/h/me"), "~/x");
        assert_eq!(tilde("/h/meow", "/h/me"), "/h/meow");
        assert_eq!(untilde("~/x", "/h/me"), "/h/me/x");
    }
}
