// Each test file uses only some of these helpers.
#![allow(dead_code)]

//! A private tmux server and a temporary HOME per test: never the user's tmux.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

static NEXT: AtomicUsize = AtomicUsize::new(0);

pub struct World {
    pub socket: String,
    home: tempfile::TempDir,
}

impl World {
    pub fn new() -> Self {
        let n = NEXT.fetch_add(1, Ordering::SeqCst);
        World {
            socket: format!("ta-test-{}-{n}", std::process::id()),
            home: tempfile::tempdir().unwrap(),
        }
    }

    /// The temporary HOME, as tmux reports paths (symlinks resolved, as
    /// macOS's `/var` → `/private/var`).
    pub fn home(&self) -> PathBuf {
        self.home.path().canonicalize().unwrap()
    }

    /// A folder under the temporary HOME, created.
    pub fn dir(&self, name: &str) -> PathBuf {
        let dir = self.home().join(name);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    pub fn ta(&self, args: &[&str], cwd: &Path) -> Output {
        self.command(args, cwd).output().unwrap()
    }

    pub fn command(&self, args: &[&str], cwd: &Path) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_ta"));
        cmd.args(args)
            .current_dir(cwd)
            .env_remove("TMUX")
            .env_remove("TMUX_PANE")
            .env("TA_TMUX_SOCKET", &self.socket)
            .env("TA_LANG", "en")
            .env("HOME", self.home());
        cmd
    }

    /// `tmux -L <private socket> <args>`; stdout.
    pub fn tmux(&self, args: &[&str]) -> String {
        let out = Command::new("tmux")
            .args(["-L", &self.socket])
            .args(args)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// Run the picker in a 120×30 session called `driver` on the private
    /// server, as a person would in a terminal; wait until it has drawn.
    pub fn start_picker(&self, cwd: &Path) {
        let ta = env!("CARGO_BIN_EXE_ta");
        let home = self.home();
        let status = Command::new("tmux")
            .args([
                "-L",
                &self.socket,
                "new-session",
                "-d",
                "-s",
                "driver",
                "-x",
                "120",
                "-y",
                "30",
                "-c",
            ])
            .arg(cwd)
            .arg("env")
            .arg(format!("TA_TMUX_SOCKET={}", self.socket))
            .arg("TA_LANG=en")
            .arg(format!("HOME={}", home.display()))
            .arg(ta)
            .status()
            .unwrap();
        assert!(status.success());
        self.wait_for_screen(|s| s.contains("Find a session"));
    }

    /// What the picker's pane shows.
    pub fn screen(&self) -> String {
        self.tmux(&["capture-pane", "-p", "-t", "driver"])
    }

    pub fn wait_for_screen(&self, ok: impl Fn(&str) -> bool) -> String {
        wait_for(|| Some(self.screen()).filter(|s| ok(s)))
    }

    /// Press keys in the picker (`Down`, `Enter`, …).
    pub fn keys(&self, keys: &[&str]) {
        for k in keys {
            self.tmux(&["send-keys", "-t", "driver", k]);
            std::thread::sleep(Duration::from_millis(60));
        }
    }

    /// Type text into the picker.
    pub fn type_text(&self, text: &str) {
        self.tmux(&["send-keys", "-t", "driver", "-l", text]);
        std::thread::sleep(Duration::from_millis(100));
    }

    pub fn session_names(&self) -> Vec<String> {
        let out = Command::new("tmux")
            .args(["-L", &self.socket, "list-sessions", "-F", "#{session_name}"])
            .output()
            .unwrap();
        let mut names: Vec<String> = String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(String::from)
            .collect();
        names.sort();
        names
    }
}

impl Drop for World {
    fn drop(&mut self) {
        // Only this test's private server.
        let _ = Command::new("tmux")
            .args(["-L", &self.socket, "kill-server"])
            .output();
        let uid = Command::new("id")
            .arg("-u")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        let base = std::env::var("TMUX_TMPDIR").unwrap_or_else(|_| "/tmp".into());
        let _ = std::fs::remove_file(
            Path::new(&base)
                .join(format!("tmux-{uid}"))
                .join(&self.socket),
        );
    }
}

pub fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

pub fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// `f`'s answer, once it has one; panics after 10 seconds.
pub fn wait_for<T>(mut f: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(v) = f() {
            return v;
        }
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(50));
    }
}
