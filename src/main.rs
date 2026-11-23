//! `ta`: find, start and switch tmux sessions.

#[macro_use]
mod i18n;
mod command;
mod config;
mod direct;
mod enter;
mod naming;
mod pick;
mod tmux;

use std::process::ExitCode;

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use unicode_width::UnicodeWidthStr;

use direct::Target;
use tmux::{Access, DuplicateName, NotInstalled, ProcessRunner, SessionInfo, Socket, Tmux};

/// Find, start and switch tmux sessions.
#[derive(Parser)]
#[command(name = "ta", version)]
struct Cli {
    /// The tmux server: a socket name (`tmux -L`) or path (`tmux -S`). Default: tmux's own.
    #[arg(long, env = "TA_TMUX_SOCKET", global = true, value_name = "NAME|PATH")]
    socket: Option<String>,

    /// Start or find the session, print its name, but don't go there.
    #[arg(short, long, global = true)]
    detached: bool,

    #[command(subcommand)]
    command: Option<Cmd>,

    /// Go to the session with this name, starting it in this folder if there is none.
    /// (`ta -- ls` for a session named like a command.)
    name: Option<String>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Start a new session in this folder, named after it unless you give a name.
    New { name: Option<String> },
    /// List sessions.
    Ls,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let config = config::load();
    i18n::set_config_language(config.as_ref().ok().and_then(|c| c.language.clone()));
    match config.and_then(|c| run(cli, &c)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ta: {}", explain(&e));
            ExitCode::FAILURE
        }
    }
}

fn explain(e: &anyhow::Error) -> String {
    if e.is::<NotInstalled>() {
        return tr!(
            "tmux is not installed. Install it with `brew install tmux` (macOS) or `sudo apt install tmux` (Debian, Ubuntu).".to_string(),
            "没有找到 tmux。先安装：`brew install tmux`（macOS）或 `sudo apt install tmux`（Debian、Ubuntu）。".to_string()
        );
    }
    format!("{e:#}")
}

fn run(cli: Cli, config: &config::Config) -> Result<()> {
    let socket = Socket::from_config(config::socket(cli.socket.as_deref(), config).as_deref());
    let tmux = Tmux::with_socket(socket.clone());
    tmux.installed()?;
    let host = config.host_prefix.then(naming::host_prefix).flatten();
    match (cli.command, cli.name) {
        (Some(_), Some(name)) => bail!(tr!(
            format!("give a session name or a command, not both (a session named like a command: ta -- {name})"),
            format!("会话名和命令只能给一个（会话名和命令同名时：ta -- {name}）")
        )),
        (Some(Cmd::Ls), None) => ls(&tmux),
        (Some(Cmd::New { name }), None) => {
            let cwd = cwd()?;
            let desired = naming::new_session_name(name.as_deref(), &cwd, host.as_deref());
            let (id, name) = create(&tmux, &desired, &cwd)?;
            arrive(&tmux, &socket, &id, &name, cli.detached)
        }
        (None, Some(wanted)) => {
            let (id, name) = match direct::resolve(&tmux.sessions(), &wanted, host.as_deref()) {
                Target::Existing(id) => {
                    let name = tmux
                        .sessions()
                        .into_iter()
                        .find(|s| s.id == id)
                        .map_or(wanted, |s| s.name);
                    (id, name)
                }
                Target::Create(name) => create(&tmux, &name, &cwd()?)?,
            };
            arrive(&tmux, &socket, &id, &name, cli.detached)
        }
        (None, None) => pick::run(&tmux, &socket, host.as_deref()),
    }
}

fn cwd() -> Result<String> {
    Ok(std::env::current_dir()?.to_string_lossy().into_owned())
}

/// Start a session named `desired` (or `desired-2`…) in `cwd`.
fn create(tmux: &Tmux<ProcessRunner>, desired: &str, cwd: &str) -> Result<(String, String)> {
    if tmux.probe_access(cwd) == Access::Denied {
        bail!(tr!(
            format!("the tmux server cannot read {cwd} (macOS privacy settings), so a new session would start in the wrong folder. Restarting tmux fixes it, but ends every session on that server; ta won't do that for you."),
            format!("tmux server 读不了 {cwd}（macOS 隐私权限），新会话会落在错误的目录。重启 tmux 能修好，但会结束这个 server 上的全部会话，ta 不会替你做。")
        ));
    }
    let mut taken: Vec<String> = tmux.sessions().into_iter().map(|s| s.name).collect();
    for _ in 0..5 {
        let name = naming::unique(desired, &taken, None);
        match tmux.new_session(&name, cwd) {
            Ok(id) => return Ok((id, name)),
            Err(e) if e.is::<DuplicateName>() => taken.push(name),
            Err(e) => return Err(e),
        }
    }
    bail!("tmux kept refusing the name {desired}")
}

fn arrive(
    tmux: &Tmux<ProcessRunner>,
    socket: &Socket,
    id: &str,
    name: &str,
    detached: bool,
) -> Result<()> {
    if detached {
        println!("{name}");
        return Ok(());
    }
    enter::enter(tmux, socket, id)
}

fn ls(tmux: &Tmux<ProcessRunner>) -> Result<()> {
    let mut sessions = tmux.sessions();
    if sessions.is_empty() {
        println!(
            "{}",
            tr!(
                "No sessions yet. Start one: ta <name>",
                "还没有会话。新建一个：ta <名字>"
            )
        );
        return Ok(());
    }
    sessions.sort_by_key(|s| std::cmp::Reverse(s.activity_ms));
    let home = std::env::var("HOME").unwrap_or_default();
    print!("{}", table(&sessions, &home));
    Ok(())
}

fn table(sessions: &[SessionInfo], home: &str) -> String {
    let width = sessions.iter().map(|s| s.name.width()).max().unwrap_or(0);
    let mut out = String::new();
    for s in sessions {
        let pad = " ".repeat(width - s.name.width());
        let path = match s.path.strip_prefix(home) {
            Some(rest) if !home.is_empty() => format!("~{rest}"),
            _ => s.path.clone(),
        };
        let mark = if s.attached { "*" } else { " " };
        let windows = tr!(format!("{} win", s.windows), format!("{} 窗口", s.windows));
        out.push_str(&format!("{mark} {}{pad}  {windows:<8} {path}\n", s.name));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_aligns_and_shortens_home() {
        i18n::set_for_test(i18n::Lang::En);
        let s = |name: &str, path: &str, attached| SessionInfo {
            id: "$1".into(),
            name: name.into(),
            windows: 2,
            activity_ms: 0,
            attached,
            path: path.into(),
        };
        let out = table(
            &[s("api", "/h/me/w", true), s("接口", "/srv", false)],
            "/h/me",
        );
        assert_eq!(out, "* api   2 win    ~/w\n  接口  2 win    /srv\n");
    }
}
