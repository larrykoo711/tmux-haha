//! Getting the terminal into a session: switch when already inside tmux,
//! attach otherwise. Adapted from cc-moss (MIT,
//! https://github.com/larrykoo711/cc-moss), `cc-moss/src/cmd/attach.rs`.

use std::os::unix::process::CommandExt;
use std::process::Command;

use anyhow::{bail, Context, Result};

use crate::tmux::{Runner, Socket, Tmux};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Enter {
    /// Inside tmux on the same server: `switch-client`.
    Switch,
    /// Outside tmux: `attach-session`.
    Attach,
    /// Inside tmux, but on another server: a client cannot switch across servers.
    OtherServer,
}

/// How to get there, given the `TMUX` value this process inherited.
pub fn decide(tmux_env: Option<&str>, same_server: impl FnOnce(&str) -> bool) -> Enter {
    match tmux_env.filter(|v| !v.is_empty()) {
        None => Enter::Attach,
        Some(client) if same_server(client) => Enter::Switch,
        Some(_) => Enter::OtherServer,
    }
}

pub fn other_server_hint() -> String {
    tr!(
        "You are inside tmux on another server. Detach first (prefix + d), then run ta again."
            .to_string(),
        "你现在在另一个 tmux server 里，不能直接切过去。先退出（prefix + d），再运行 ta。"
            .to_string()
    )
}

/// Bring the terminal to session `id` (`$N`).
pub fn enter<R: Runner>(tmux: &Tmux<R>, socket: &Socket, id: &str) -> Result<()> {
    let env = std::env::var("TMUX").ok();
    let mut cmd = Command::new("tmux");
    cmd.args(socket.args());
    match decide(env.as_deref(), |client| tmux.matches_client(client)) {
        Enter::OtherServer => bail!(other_server_hint()),
        Enter::Switch => {
            let status = cmd
                .args(["switch-client", "-t", id])
                .status()
                .context("tmux switch-client")?;
            anyhow::ensure!(status.success(), "tmux switch-client failed");
            Ok(())
        }
        Enter::Attach => {
            cmd.args(["attach-session", "-t", id]);
            Err(cmd.exec()).context("tmux attach-session")
        }
    }
}

/// Bring the terminal to window `window` (`@N`) of session `session`: make
/// it the session's current window, then go to the session.
pub fn enter_window<R: Runner>(
    tmux: &Tmux<R>,
    socket: &Socket,
    session: &str,
    window: &str,
) -> Result<()> {
    tmux.select_window(window)?;
    enter(tmux, socket, session)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outside_tmux_attaches() {
        assert_eq!(decide(None, |_| unreachable!()), Enter::Attach);
        assert_eq!(decide(Some(""), |_| unreachable!()), Enter::Attach);
    }

    #[test]
    fn same_server_switches() {
        assert_eq!(
            decide(Some("/tmp/tmux-1/default,42,0"), |_| true),
            Enter::Switch
        );
    }

    #[test]
    fn inside_another_server_is_refused_with_a_hint() {
        assert_eq!(
            decide(Some("/tmp/tmux-1/other,42,0"), |_| false),
            Enter::OtherServer
        );
        crate::i18n::set_for_test(crate::i18n::Lang::En);
        assert!(other_server_hint().contains("Detach first"));
        crate::i18n::set_for_test(crate::i18n::Lang::Zh);
        assert!(other_server_hint().contains("先退出"));
    }
}
