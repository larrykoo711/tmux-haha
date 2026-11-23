//! `ta`: find, start and switch tmux sessions.

#[macro_use]
mod i18n;
mod block;
mod cli;
mod command;
mod commands;
mod config;
mod confirm;
mod direct;
mod doctor;
mod enter;
mod folders;
mod help_zh;
mod install;
mod naming;
mod pick;
mod settings;
mod tmux;
mod update;
mod version;

use std::process::ExitCode;

use anyhow::{bail, Result};
use unicode_width::UnicodeWidthStr;

use cli::{Cli, Cmd};
use direct::Target;
use tmux::{Access, DuplicateName, NotInstalled, ProcessRunner, SessionInfo, Socket, Tmux};

fn main() -> ExitCode {
    // The config first: its `language` decides the help's language too.
    let config = config::load();
    i18n::set_config_language(config.as_ref().ok().and_then(|c| c.language.clone()));
    let cli = cli::parse();
    if cli.version || matches!(cli.command, Some(Cmd::Version)) {
        version::print();
        return ExitCode::SUCCESS;
    }
    // These two work with a broken config: they are how you find what's wrong.
    let checked = match &cli.command {
        Some(Cmd::Doctor) => Some(doctor::run(
            &Tmux::with_socket(Socket::from_config(
                config::socket(
                    cli.socket.as_deref(),
                    config.as_ref().unwrap_or(&config::Config::default()),
                )
                .as_deref(),
            )),
            &config,
        )),
        Some(Cmd::Config { path }) => Some(settings::show(cli.socket.as_deref(), &config, *path)),
        Some(Cmd::Completions { shell }) => {
            print!("{}", install::completions(*shell));
            Some(Ok(true))
        }
        // Needs no tmux and no config.
        Some(Cmd::Update { check, yes }) => Some(update::run(*check, *yes).map(|()| true)),
        _ => None,
    };
    if let Some(result) = checked {
        return match result {
            Ok(true) => ExitCode::SUCCESS,
            Ok(false) => ExitCode::FAILURE,
            Err(e) => {
                eprintln!("ta: {}", explain(&e));
                ExitCode::FAILURE
            }
        };
    }
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
        (Some(Cmd::Ls { json: true }), None) => commands::ls_json(&tmux),
        (Some(Cmd::Ls { json: false }), None) => ls(&tmux),
        (Some(Cmd::Close { name, yes }), None) => commands::close(&tmux, &name, yes, host.as_deref()),
        (Some(Cmd::Rename { name, new_name }), None) => {
            commands::rename(&tmux, &name, &new_name, host.as_deref())
        }
        // Printed before tmux is needed (`main`).
        (
            Some(
                Cmd::Version
                | Cmd::Doctor
                | Cmd::Config { .. }
                | Cmd::Update { .. }
                | Cmd::Completions { .. },
            ),
            None,
        ) => Ok(()),
        (
            Some(Cmd::Install {
                yes,
                uninstall,
                shell,
                no_tmux_key,
            }),
            None,
        ) => install::run(
            &tmux,
            install::Options {
                yes,
                uninstall,
                shell,
                no_tmux_key,
            },
        ),
        (Some(Cmd::New { name, dir }), None) => {
            let cwd = match dir {
                Some(dir) => commands::folder(&dir)?,
                None => cwd()?,
            };
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
        (None, None) => {
            let home = std::env::var("HOME").unwrap_or_default();
            pick::run(&tmux, &socket, host.as_deref(), &config.roots(&home))
        }
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
        let tool = s
            .managed_by
            .as_deref()
            .map(|t| format!("  ({t})"))
            .unwrap_or_default();
        out.push_str(&format!(
            "{mark} {}{pad}  {windows:<8} {path}{tool}\n",
            s.name
        ));
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
            managed_by: None,
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

    #[test]
    fn table_shows_who_manages_a_session() {
        i18n::set_for_test(i18n::Lang::En);
        let s = SessionInfo {
            managed_by: Some("cc-moss".into()),
            id: "$1".into(),
            name: "work".into(),
            windows: 1,
            activity_ms: 0,
            attached: false,
            path: "/srv".into(),
        };
        assert_eq!(table(&[s], "/h"), "  work  1 win    /srv  (cc-moss)\n");
    }
}
