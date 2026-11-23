//! tmux adapter. All commands go through a [`Runner`] so they can be tested
//! without a tmux server. Adapted from cc-moss (MIT,
//! https://github.com/larrykoo711/cc-moss), `moss-core/src/tmux.rs`.
//!
//! Never sets global options or key bindings and never ends the server:
//! the tmux server may be shared with other tools.

use std::process::Command;
use std::time::Duration;

use anyhow::{bail, Result};

use crate::command::{run_with_timeout, RunError};

/// How long a command a person waits on may take.
pub const TIMEOUT: Duration = Duration::from_secs(5);

/// tmux says so when a session name is already taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DuplicateName;

impl std::fmt::Display for DuplicateName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("tmux new-session: duplicate session name")
    }
}
impl std::error::Error for DuplicateName {}

/// tmux is not installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotInstalled;

impl std::fmt::Display for NotInstalled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("tmux is not installed")
    }
}
impl std::error::Error for NotInstalled {}

/// `tmux -u`: without a UTF-8 locale tmux otherwise prints each non-ASCII
/// letter of a name as `_`. Interactive attaching keeps plain tmux.
pub fn command() -> Command {
    let mut cmd = Command::new("tmux");
    cmd.arg("-u");
    cmd
}

pub trait Runner {
    /// Run `tmux <args>` and return stdout.
    fn run(&self, args: &[String]) -> Result<String>;
}

/// Which tmux server to talk to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Socket {
    Default,
    /// `tmux -L <name>`
    Name(String),
    /// `tmux -S <path>`
    Path(String),
}

impl Socket {
    /// A configured value: paths contain `/`, anything else is a name.
    pub fn from_config(value: Option<&str>) -> Self {
        match value {
            None | Some("") => Socket::Default,
            Some(v) if v.contains('/') => Socket::Path(v.to_string()),
            Some(v) => Socket::Name(v.to_string()),
        }
    }

    pub fn args(&self) -> Vec<String> {
        match self {
            Socket::Default => vec![],
            Socket::Name(n) => vec!["-L".into(), n.clone()],
            Socket::Path(p) => vec!["-S".into(), p.clone()],
        }
    }
}

pub struct ProcessRunner {
    pub socket: Socket,
    pub timeout: Duration,
}

impl Runner for ProcessRunner {
    fn run(&self, args: &[String]) -> Result<String> {
        let mut all = self.socket.args();
        all.extend_from_slice(args);
        let what = args.first().map_or("", |s| s.as_str());
        let out = match run_with_timeout(command().args(&all), self.timeout) {
            Ok(out) => out,
            Err(e) if e.is_not_installed() => return Err(NotInstalled.into()),
            Err(RunError::NotStarted(e)) => bail!("tmux {what} could not start: {e}"),
            Err(RunError::TimedOut) => bail!("tmux {what} did not finish"),
        };
        if !out.status.success() {
            let why = String::from_utf8_lossy(&out.stderr);
            let why = why.trim();
            if what == "new-session"
                && args
                    .windows(2)
                    .any(|pair| pair[0] == "-s" && why == format!("duplicate session: {}", pair[1]))
            {
                return Err(DuplicateName.into());
            }
            if why.is_empty() {
                bail!("tmux {what} failed");
            }
            bail!("tmux {what} failed: {why}");
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }
}

pub struct Tmux<R: Runner> {
    runner: R,
}

impl Tmux<ProcessRunner> {
    pub fn with_socket(socket: Socket) -> Self {
        Tmux::new(ProcessRunner {
            socket,
            timeout: TIMEOUT,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Readable,
    Denied,
    NoServer,
}

/// One row of `tmux list-sessions`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionInfo {
    pub id: String,
    pub name: String,
    pub windows: u32,
    pub activity_ms: i64,
    pub attached: bool,
    /// The session's start directory.
    pub path: String,
}

/// Field separator in `-F` formats. Printable on purpose: tmux 3.5 replaces
/// control characters such as tabs in format output with `_`.
const SEP: &str = "|:|";

const SESSION_FORMAT: &str = "#{session_id}|:|#{session_name}|:|#{session_windows}|:|#{session_activity}|:|#{session_attached}|:|#{session_path}";

fn parse_session(line: &str) -> Option<SessionInfo> {
    let mut f = line.splitn(6, SEP);
    Some(SessionInfo {
        id: f.next()?.to_string(),
        name: f.next()?.to_string(),
        windows: f.next()?.parse().ok()?,
        activity_ms: f.next()?.parse::<i64>().ok()? * 1000,
        attached: f.next()? != "0",
        path: f.next().unwrap_or_default().to_string(),
    })
}

/// tmux expands `#` formats in `-c` and treats a trailing `;` in any argument
/// as a command separator; neither must change what reaches tmux.
fn escape_cwd(cwd: &str) -> String {
    let cwd = cwd.replace('#', "##");
    match cwd.strip_suffix(';') {
        Some(head) => format!("{head}\\;"),
        None => cwd,
    }
}

impl<R: Runner> Tmux<R> {
    pub fn new(runner: R) -> Self {
        Tmux { runner }
    }

    fn run(&self, args: &[&str]) -> Result<String> {
        self.runner
            .run(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    /// Whether tmux can be run at all.
    pub fn installed(&self) -> Result<()> {
        match self.run(&["-V"]) {
            Err(e) if e.is::<NotInstalled>() => Err(e),
            _ => Ok(()),
        }
    }

    /// A detached session running the user's shell in `cwd`. Returns its id (`$N`).
    pub fn new_session(&self, name: &str, cwd: &str) -> Result<String> {
        let cwd = escape_cwd(cwd);
        let out = self.run(&[
            "new-session",
            "-d",
            "-P",
            "-F",
            "#{session_id}",
            "-s",
            name,
            "-c",
            &cwd,
        ])?;
        match out.lines().next().map(str::trim) {
            Some(id) if id.starts_with('$') => Ok(id.to_string()),
            _ => bail!("tmux new-session did not say which session it made"),
        }
    }

    /// Whether the running server can read `dir`. A long-running tmux server
    /// on macOS can lose access to protected folders (Documents, Desktop…)
    /// that a fresh terminal still has; sessions it starts there then land in
    /// the wrong directory. No server means a new one is started by us.
    pub fn probe_access(&self, dir: &str) -> Access {
        if self.run(&["list-sessions", "-F", "#{session_id}"]).is_err() {
            return Access::NoServer;
        }
        let q = format!("'{}'", dir.replace('\'', r"'\''"));
        match self.run(&["run-shell", &format!("test -r {q} && test -x {q}")]) {
            Ok(_) => Access::Readable,
            Err(_) => Access::Denied,
        }
    }

    /// Whether `client` (the `TMUX` value: socket path, server pid, …) is this server.
    pub fn matches_client(&self, client: &str) -> bool {
        let mut parts = client.split(',');
        let Some(socket) = parts.next().filter(|value| !value.is_empty()) else {
            return false;
        };
        let Some(pid) = parts.next().and_then(|value| value.parse::<u32>().ok()) else {
            return false;
        };
        let Ok(server) = self.run(&["display-message", "-p", "#{socket_path}|:|#{pid}"]) else {
            return false;
        };
        server
            .trim()
            .split_once(SEP)
            .is_some_and(|(path, process)| {
                path == socket && process.parse::<u32>().ok() == Some(pid)
            })
    }

    /// Every session; empty when no server runs.
    pub fn sessions(&self) -> Vec<SessionInfo> {
        let Ok(out) = self.run(&["list-sessions", "-F", SESSION_FORMAT]) else {
            return Vec::new();
        };
        out.lines().filter_map(parse_session).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_values() {
        assert_eq!(Socket::from_config(None), Socket::Default);
        assert_eq!(Socket::from_config(Some("")), Socket::Default);
        assert_eq!(Socket::from_config(Some("work")).args(), ["-L", "work"]);
        assert_eq!(Socket::from_config(Some("/tmp/s")).args(), ["-S", "/tmp/s"]);
    }

    #[test]
    fn parses_a_session_row() {
        let row = parse_session("$3|:|api|:|2|:|1700000000|:|1|:|/w/a|:|b").unwrap();
        assert_eq!(row.id, "$3");
        assert_eq!(row.name, "api");
        assert_eq!(row.windows, 2);
        assert_eq!(row.activity_ms, 1_700_000_000_000);
        assert!(row.attached);
        assert_eq!(row.path, "/w/a|:|b");
        assert!(parse_session("garbage").is_none());
    }

    #[test]
    fn cwd_cannot_inject_formats_or_commands() {
        assert_eq!(escape_cwd("/w/#{pid}"), "/w/##{pid}");
        assert_eq!(escape_cwd("/w/x;"), "/w/x\\;");
    }
}
