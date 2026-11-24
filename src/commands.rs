//! Session commands for scripts: the picker's actions, same rules.

use std::path::Path;

use anyhow::{bail, Result};
use serde_json::json;

use crate::direct;
use crate::naming;
use crate::tmux::{ProcessRunner, SessionInfo, Tmux};

/// `ta ls --json`: every session, for machines.
pub fn ls_json(tmux: &Tmux<ProcessRunner>) -> Result<()> {
    let sessions: Vec<_> = tmux
        .sessions()
        .into_iter()
        .map(|s| {
            json!({
                "id": s.id,
                "name": s.name,
                "windows": s.windows,
                "attached": s.attached,
                "activity": s.activity_ms / 1000,
                "path": s.path,
                "managed_by": s.managed_by,
                "command": s.command,
                "title": s.title,
            })
        })
        .collect();
    println!("{}", json!({ "schema": 1, "sessions": sessions }));
    Ok(())
}

/// The session `wanted` names, or a message saying there is none.
fn session(tmux: &Tmux<ProcessRunner>, wanted: &str, host: Option<&str>) -> Result<SessionInfo> {
    if wanted.trim().is_empty() {
        bail!(tr!("give a session name", "请给出会话名"));
    }
    match direct::find(&tmux.sessions(), wanted, host) {
        Some(s) => Ok(s.clone()),
        None => bail!(tr!(
            format!("there is no session called {wanted} (ta ls lists them)"),
            format!("没有叫 {wanted} 的会话（ta ls 可以列出全部）")
        )),
    }
}

/// Why `s` must be left alone, if it must: another tool manages it.
fn managed(s: &SessionInfo) -> Result<()> {
    if let Some(tool) = &s.managed_by {
        bail!(crate::pick::app::managed_hint(tool));
    }
    Ok(())
}

/// `ta close <name>`: end a session, after asking (or with `yes`).
pub fn close(
    tmux: &Tmux<ProcessRunner>,
    wanted: &str,
    yes: bool,
    host: Option<&str>,
) -> Result<()> {
    let s = session(tmux, wanted, host)?;
    managed(&s)?;
    let here = tmux.current_session(
        std::env::var("TMUX").ok().as_deref(),
        std::env::var("TMUX_PANE").ok().as_deref(),
    );
    if here.as_deref() == Some(s.id.as_str()) {
        bail!(crate::pick::app::here_hint());
    }
    if !yes && !confirm(&s.name)? {
        println!("{}", tr!("Kept it.", "没有关闭。"));
        return Ok(());
    }
    tmux.kill_session(&s.id)?;
    println!(
        "{}",
        tr!(
            format!("Closed {}.", s.name),
            format!("已关闭 {}。", s.name)
        )
    );
    Ok(())
}

fn confirm(name: &str) -> Result<bool> {
    crate::confirm::ask(
        false,
        &tr!(
            format!("Close {name}? It ends everything running in it."),
            format!("关闭 {name}？会结束里面运行的一切。")
        ),
        tr!("add --yes to close it without asking", "不问就关请加 --yes"),
        false,
    )
}

/// `ta rename <name> <new>`: rename it, the new name made one tmux takes.
pub fn rename(
    tmux: &Tmux<ProcessRunner>,
    wanted: &str,
    new: &str,
    host: Option<&str>,
) -> Result<()> {
    let s = session(tmux, wanted, host)?;
    managed(&s)?;
    let name = rename_session(tmux, &s.id, new)?;
    println!("{name}");
    Ok(())
}

/// Rename session `$N` to `wanted`, as tmux can take it and unique on the
/// server; the name it got.
pub fn rename_session(tmux: &Tmux<ProcessRunner>, id: &str, wanted: &str) -> Result<String> {
    let sessions = tmux.sessions();
    let current = sessions.iter().find(|s| s.id == id).map(|s| s.name.clone());
    let taken: Vec<String> = sessions.into_iter().map(|s| s.name).collect();
    let name = naming::unique(&naming::sanitize(wanted), &taken, current.as_deref());
    tmux.rename_session(id, &name)?;
    Ok(name)
}

/// The folder `ta new --dir` starts in: an existing folder, absolute.
pub fn folder(dir: &Path) -> Result<String> {
    match std::fs::canonicalize(dir) {
        Ok(p) if p.is_dir() => Ok(p.to_string_lossy().into_owned()),
        _ => bail!(tr!(
            format!("{} is not a folder", dir.display()),
            format!("{} 不是一个目录", dir.display())
        )),
    }
}
