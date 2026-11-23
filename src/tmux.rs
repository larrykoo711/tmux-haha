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
    /// The tool that manages it (`@managed-by`), when one says so: ta only
    /// goes to such a session, never renames or closes it.
    pub managed_by: Option<String>,
    /// The session's start directory.
    pub path: String,
}

/// One row of `tmux list-windows -a`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowInfo {
    pub session_id: String,
    pub session_name: String,
    /// `@N`
    pub id: String,
    pub index: u32,
    pub name: String,
    /// The session's current window.
    pub active: bool,
    pub panes: u32,
    pub activity_ms: i64,
    /// Where its active pane is now.
    pub path: String,
}

/// Field separator in `-F` formats. Printable on purpose: tmux 3.5 replaces
/// control characters such as tabs in format output with `_`.
const SEP: &str = "|:|";

/// `#{@managed-by}`: the option a tool sets on the sessions it manages (see the README).
// Free text (names, paths) can hold anything, the separator included: it
// is only ever the last field of a line, after fields that can't hold it,
// and names and paths are asked for in their own calls, keyed by id.
const SESSION_FORMAT: &str = "#{session_id}|:|#{session_windows}|:|#{session_activity}|:|#{session_attached}|:|#{@managed-by}|:|#{session_name}";
const SESSION_PATHS: &str = "#{session_id}|:|#{session_path}";
const WINDOW_FORMAT: &str = "#{session_id}|:|#{window_id}|:|#{window_index}|:|#{window_active}|:|#{window_panes}|:|#{window_activity}|:|#{window_name}";
const WINDOW_PATHS: &str = "#{window_id}|:|#{pane_current_path}";

/// `$3`, `@12`: tmux's own ids, nothing else.
fn is_id(s: &str, sigil: char) -> bool {
    s.strip_prefix(sigil)
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// One `list-windows` line; the session's name and the path come separately.
fn parse_window(line: &str) -> Option<WindowInfo> {
    let mut f = line.splitn(7, SEP);
    let session_id = f.next()?.to_string();
    let id = f.next()?.to_string();
    if !is_id(&session_id, '$') || !is_id(&id, '@') {
        return None;
    }
    Some(WindowInfo {
        session_id,
        session_name: String::new(),
        id,
        index: f.next()?.parse().ok()?,
        active: f.next()? == "1",
        panes: f.next()?.parse().ok()?,
        activity_ms: f.next()?.parse::<i64>().ok()? * 1000,
        name: f.next()?.to_string(),
        path: String::new(),
    })
}

/// One `list-sessions` line; the path comes separately.
fn parse_session(line: &str) -> Option<SessionInfo> {
    let mut f = line.splitn(6, SEP);
    let id = f.next()?.to_string();
    if !is_id(&id, '$') {
        return None;
    }
    Some(SessionInfo {
        id,
        windows: f.next()?.parse().ok()?,
        activity_ms: f.next()?.parse::<i64>().ok()? * 1000,
        attached: f.next()? != "0",
        managed_by: Some(f.next()?.trim().to_string()).filter(|m| !m.is_empty()),
        name: f.next()?.to_string(),
        path: String::new(),
    })
}

/// `id → free text` from lines of `<id>|:|<text>`.
fn by_id(out: &str) -> std::collections::HashMap<String, String> {
    out.lines()
        .filter_map(|l| l.split_once(SEP))
        .map(|(id, text)| (id.to_string(), text.to_string()))
        .collect()
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

    /// `tmux -V` (e.g. `tmux 3.6`); `None` when tmux can't be run.
    pub fn version(&self) -> Option<String> {
        self.run(&["-V"])
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
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
        // tmux expands `#{…}` and runs `#(…)` in a run-shell command before
        // the shell sees it: `##` is a literal `#`. Then quote for the shell.
        let q = format!("'{}'", dir.replace('#', "##").replace('\'', r"'\''"));
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

    /// Rename by session id (`$N`), never by name, so it cannot hit another session.
    pub fn rename_session(&self, session_id: &str, new_name: &str) -> Result<()> {
        self.run(&["rename-session", "-t", session_id, new_name])
            .map(drop)
    }

    /// End session `$N` (its programs get SIGHUP). By id, never by name.
    pub fn kill_session(&self, session_id: &str) -> Result<()> {
        self.run(&["kill-session", "-t", session_id]).map(drop)
    }

    /// The session this process runs in (`$N`), when it runs inside tmux on
    /// this server: `tmux_env` and `pane` are the inherited `TMUX` and `TMUX_PANE`.
    pub fn current_session(&self, tmux_env: Option<&str>, pane: Option<&str>) -> Option<String> {
        let tmux_env = tmux_env?;
        if !self.matches_client(tmux_env) {
            return None;
        }
        let Some(pane) = pane.filter(|p| p.starts_with('%')) else {
            // A popup (`display-popup`) has no TMUX_PANE; TMUX's third field
            // is the session it was opened over.
            let n = tmux_env.split(',').nth(2)?;
            let id = format!("${n}");
            if !is_id(&id, '$') {
                return None;
            }
            let out = self
                .run(&["display-message", "-p", "-t", &id, "#{session_id}"])
                .ok()?;
            return (out.trim_end() == id).then_some(id);
        };
        let out = self
            .run(&[
                "display-message",
                "-p",
                "-t",
                pane,
                "#{pane_id}|:|#{session_id}",
            ])
            .ok()?;
        // display-message falls back to the current pane for a gone target.
        let (id, session) = out.trim_end().split_once(SEP)?;
        (id == pane && session.starts_with('$')).then(|| session.to_string())
    }

    /// Make window `@N` its session's current window.
    pub fn select_window(&self, window_id: &str) -> Result<()> {
        self.run(&["select-window", "-t", window_id]).map(drop)
    }

    /// Every window of every session; empty when no server runs.
    pub fn windows(&self) -> Vec<WindowInfo> {
        let Ok(out) = self.run(&["list-windows", "-a", "-F", WINDOW_FORMAT]) else {
            return Vec::new();
        };
        let paths = self
            .run(&["list-windows", "-a", "-F", WINDOW_PATHS])
            .map(|o| by_id(&o))
            .unwrap_or_default();
        let names = self
            .run(&["list-sessions", "-F", "#{session_id}|:|#{session_name}"])
            .map(|o| by_id(&o))
            .unwrap_or_default();
        out.lines()
            .filter_map(parse_window)
            .map(|mut w| {
                w.path = paths.get(&w.id).cloned().unwrap_or_default();
                w.session_name = names.get(&w.session_id).cloned().unwrap_or_default();
                w
            })
            .collect()
    }

    /// Every session; empty when no server runs.
    pub fn sessions(&self) -> Vec<SessionInfo> {
        let Ok(out) = self.run(&["list-sessions", "-F", SESSION_FORMAT]) else {
            return Vec::new();
        };
        let paths = self
            .run(&["list-sessions", "-F", SESSION_PATHS])
            .map(|o| by_id(&o))
            .unwrap_or_default();
        out.lines()
            .filter_map(parse_session)
            .map(|mut s| {
                s.path = paths.get(&s.id).cloned().unwrap_or_default();
                s
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// Records each command and answers from a script, in order.
    struct Fake {
        calls: RefCell<Vec<Vec<String>>>,
        answers: RefCell<Vec<Result<String, String>>>,
    }

    impl Fake {
        fn new(answers: &[Result<&str, &str>]) -> Self {
            Fake {
                calls: RefCell::default(),
                answers: RefCell::new(
                    answers
                        .iter()
                        .rev()
                        .map(|a| a.map(String::from).map_err(String::from))
                        .collect(),
                ),
            }
        }
    }

    impl Runner for &Fake {
        fn run(&self, args: &[String]) -> Result<String> {
            self.calls.borrow_mut().push(args.to_vec());
            match self.answers.borrow_mut().pop() {
                Some(Ok(out)) => Ok(out),
                Some(Err(e)) => bail!(e),
                None => Ok(String::new()),
            }
        }
    }

    #[test]
    fn rename_and_kill_target_ids() {
        let fake = Fake::new(&[]);
        let tmux = Tmux::new(&fake);
        tmux.rename_session("$3", "new").unwrap();
        tmux.kill_session("$4").unwrap();
        let calls = fake.calls.borrow();
        assert_eq!(calls[0], ["rename-session", "-t", "$3", "new"]);
        assert_eq!(calls[1], ["kill-session", "-t", "$4"]);
    }

    #[test]
    fn current_session_from_pane() {
        let here = "/tmp/tmux-1/default,42,0";
        let fake = Fake::new(&[Ok("/tmp/tmux-1/default|:|42\n"), Ok("%7|:|$2\n")]);
        let tmux = Tmux::new(&fake);
        assert_eq!(
            tmux.current_session(Some(here), Some("%7")),
            Some("$2".into())
        );
        // Another server: not "here".
        let fake = Fake::new(&[Ok("/tmp/tmux-1/other|:|9\n")]);
        assert_eq!(
            Tmux::new(&fake).current_session(Some(here), Some("%7")),
            None
        );
        // A pane that is gone: tmux answers for another pane.
        let fake = Fake::new(&[Ok("/tmp/tmux-1/default|:|42\n"), Ok("%1|:|$0\n")]);
        assert_eq!(
            Tmux::new(&fake).current_session(Some(here), Some("%7")),
            None
        );
        // A popup: no pane, the session from TMUX's third field.
        let fake = Fake::new(&[Ok("/tmp/tmux-1/default|:|42\n"), Ok("$5\n")]);
        assert_eq!(
            Tmux::new(&fake).current_session(Some("/tmp/tmux-1/default,42,5"), None),
            Some("$5".into())
        );
        // …that session gone: tmux names another, so not "here".
        let fake = Fake::new(&[Ok("/tmp/tmux-1/default|:|42\n"), Ok("$0\n")]);
        assert_eq!(
            Tmux::new(&fake).current_session(Some("/tmp/tmux-1/default,42,5"), None),
            None
        );
        // Outside tmux.
        let fake = Fake::new(&[]);
        assert_eq!(Tmux::new(&fake).current_session(None, None), None);
        assert!(fake.calls.borrow().is_empty());
    }

    #[test]
    fn socket_values() {
        assert_eq!(Socket::from_config(None), Socket::Default);
        assert_eq!(Socket::from_config(Some("")), Socket::Default);
        assert_eq!(Socket::from_config(Some("work")).args(), ["-L", "work"]);
        assert_eq!(Socket::from_config(Some("/tmp/s")).args(), ["-S", "/tmp/s"]);
    }

    #[test]
    fn parses_a_session_row() {
        let row = parse_session("$3|:|2|:|1700000000|:|1|:||:|api|:|x").unwrap();
        assert_eq!((row.id.as_str(), row.name.as_str()), ("$3", "api|:|x"));
        assert_eq!(row.windows, 2);
        assert_eq!(row.activity_ms, 1_700_000_000_000);
        assert!(row.attached);
        assert_eq!(row.managed_by, None);
        let row = parse_session("$4|:|1|:|1700000000|:|0|:|cc-moss|:|claude").unwrap();
        assert_eq!(row.managed_by.as_deref(), Some("cc-moss"));
        assert!(parse_session("garbage").is_none());
        assert!(
            parse_session("evil|:|1|:|1|:|0|:||:|x").is_none(),
            "not an id"
        );
    }

    #[test]
    fn parses_a_window_row() {
        let w = parse_window("$1|:|@4|:|2|:|1|:|3|:|1700000000|:|vim|:|x").unwrap();
        assert_eq!((w.session_id.as_str(), w.id.as_str()), ("$1", "@4"));
        assert_eq!((w.index, w.name.as_str()), (2, "vim|:|x"));
        assert!(w.active);
        assert_eq!(w.panes, 3);
        assert_eq!(w.activity_ms, 1_700_000_000_000);
        assert!(parse_window("$1|:|api").is_none());
    }

    #[test]
    fn cwd_cannot_inject_formats_or_commands() {
        assert_eq!(escape_cwd("/w/#{pid}"), "/w/##{pid}");
        assert_eq!(escape_cwd("/w/x;"), "/w/x\\;");
    }
}
