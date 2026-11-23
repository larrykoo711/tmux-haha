//! `ta update`, offline: a fake `curl` plays GitHub and a fake `brew` logs
//! what it was asked to do. The program being replaced is a copy
//! (`TA_UPDATE_EXE`), never the test binary.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const RELEASES: &str = "https://example.invalid/tmux-haha/releases";
const NEW: &str = "9.9.9";

struct Env {
    dir: tempfile::TempDir,
}

fn write_exe(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn target() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("linux", "x86_64") => "x86_64-unknown-linux-musl",
        _ => "aarch64-unknown-linux-musl",
    }
}

impl Env {
    fn new() -> Env {
        let env = Env {
            dir: tempfile::tempdir().unwrap(),
        };
        let fake = env.path("fake");
        write_exe(
            &env.path("bin/curl"),
            &format!(
                r#"#!/bin/sh
echo "$*" >> "{fake}/curl.log"
case " $* " in
  *" -I "*)
    if [ -f "{fake}/latest" ]; then
      printf 'HTTP/2 302\r\nlocation: {RELEASES}/tag/v%s\r\n' "$(cat "{fake}/latest")"; exit 0
    fi
    echo "curl: (6) Could not resolve host" >&2; exit 6 ;;
esac
out=""; prev=""; for a in "$@"; do [ "$prev" = "-o" ] && out="$a"; prev="$a"; url="$a"; done
file="{fake}/files/$(basename "$url")"
[ -f "$file" ] || {{ echo "curl: (22) 404" >&2; exit 22; }}
cp "$file" "$out"
"#,
                fake = fake.display()
            ),
        );
        write_exe(
            &env.path("bin/brew"),
            &format!(
                "#!/bin/sh\necho \"$*\" >> \"{fake}/brew.log\"\nprintf '#!/bin/sh\\necho \"ta {NEW}\"\\n' > \"{bin}\"\nchmod 755 \"{bin}\"\n",
                fake = fake.display(),
                bin = env.path("brew-prefix/bin/ta").display()
            ),
        );
        std::fs::create_dir_all(env.path("fake/files")).unwrap();
        std::fs::create_dir_all(env.path("brew-prefix/bin")).unwrap();
        env
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.dir.path().join(rel)
    }

    fn serve_latest(&self, version: &str) {
        std::fs::write(self.path("fake/latest"), version).unwrap();
    }

    /// A release archive for this machine whose `ta` says `ta <version>`.
    fn serve_release(&self, version: &str, good_checksum: bool) {
        let name = format!("tmux-haha-v{version}-{}", target());
        let build = self.path(&format!("build/{name}"));
        write_exe(
            &build.join("ta"),
            &format!("#!/bin/sh\necho \"ta {version}\"\n"),
        );
        let tarball = self.path(&format!("fake/files/{name}.tar.gz"));
        let ok = Command::new("tar")
            .arg("-czf")
            .arg(&tarball)
            .arg("-C")
            .arg(self.path("build"))
            .arg(&name)
            .status()
            .unwrap();
        assert!(ok.success());
        let out = Command::new("shasum")
            .args(["-a", "256"])
            .arg(&tarball)
            .output()
            .unwrap();
        let mut sum = String::from_utf8_lossy(&out.stdout)
            .split_whitespace()
            .next()
            .unwrap()
            .to_string();
        if !good_checksum {
            sum = "0".repeat(64);
        }
        std::fs::write(
            format!("{}.sha256", tarball.display()),
            format!("{sum}  {name}.tar.gz\n"),
        )
        .unwrap();
    }

    fn run(&self, exe: &Path, args: &[&str]) -> Output {
        let path = format!(
            "{}:{}",
            self.path("bin").display(),
            std::env::var("PATH").unwrap()
        );
        Command::new(env!("CARGO_BIN_EXE_ta"))
            .args(args)
            .env("PATH", path)
            .env("TA_RELEASES_URL", RELEASES)
            .env("TA_UPDATE_EXE", exe)
            .env("TA_LANG", "en")
            .env("HOME", self.dir.path())
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("TMUX")
            .output()
            .unwrap()
    }

    fn log(&self, tool: &str) -> String {
        std::fs::read_to_string(self.path(&format!("fake/{tool}.log"))).unwrap_or_default()
    }
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// A stand-in for an installed ta: a script that says the old version.
fn old_ta(env: &Env, rel: &str) -> PathBuf {
    let exe = env.path(rel);
    write_exe(&exe, "#!/bin/sh\necho \"ta 0.0.1\"\n");
    exe
}

#[test]
fn check_reports_and_changes_nothing() {
    let env = Env::new();
    env.serve_latest(NEW);
    let exe = old_ta(&env, "local/bin/ta");
    let before = std::fs::read(&exe).unwrap();
    let out = env.run(&exe, &["update", "--check"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(
        text(&out).contains(&format!("ta {NEW} is out")),
        "{}",
        text(&out)
    );
    assert!(
        text(&out).contains(&format!("{RELEASES}/tag/v{NEW}")),
        "{}",
        text(&out)
    );
    assert_eq!(std::fs::read(&exe).unwrap(), before);
    let calls = env.log("curl");
    assert_eq!(calls.lines().count(), 1, "{calls}");
    assert!(
        calls.contains("-I") && calls.contains("--proto =https") && calls.contains("--tlsv1.2"),
        "{calls}"
    );
}

#[test]
fn already_latest() {
    let env = Env::new();
    env.serve_latest("0.0.1");
    let exe = old_ta(&env, "local/bin/ta");
    let out = env.run(&exe, &["update", "--yes"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(
        text(&out).contains("You already have the latest ta"),
        "{}",
        text(&out)
    );
}

#[test]
fn a_download_replaces_itself() {
    let env = Env::new();
    env.serve_latest(NEW);
    env.serve_release(NEW, true);
    let exe = old_ta(&env, "local/bin/ta");
    let out = env.run(&exe, &["update", "--yes"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(text(&out).contains("Checksum matches."), "{}", text(&out));
    let now = Command::new(&exe).output().unwrap();
    assert_eq!(
        String::from_utf8_lossy(&now.stdout).trim(),
        format!("ta {NEW}")
    );
    // The work folder is gone.
    let left: Vec<_> = std::fs::read_dir(env.path("local/bin")).unwrap().collect();
    assert_eq!(left.len(), 1, "{left:?}");
}

#[test]
fn a_bad_checksum_leaves_ta_unchanged() {
    let env = Env::new();
    env.serve_latest(NEW);
    env.serve_release(NEW, false);
    let exe = old_ta(&env, "local/bin/ta");
    let before = std::fs::read(&exe).unwrap();
    let out = env.run(&exe, &["update", "--yes"]);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(
        text(&out).contains("does not match its published checksum"),
        "{}",
        text(&out)
    );
    assert!(
        text(&out).contains("your ta is unchanged"),
        "{}",
        text(&out)
    );
    assert_eq!(std::fs::read(&exe).unwrap(), before);
}

#[test]
fn offline_says_so() {
    let env = Env::new();
    let exe = old_ta(&env, "local/bin/ta");
    let out = env.run(&exe, &["update", "--check"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        text(&out).contains("could not check for a new version"),
        "{}",
        text(&out)
    );
}

#[test]
fn homebrew_installs_upgrade_with_brew() {
    let env = Env::new();
    env.serve_latest(NEW);
    let exe = old_ta(&env, "brew-prefix/Cellar/tmux-haha/0.0.1/bin/ta");
    let before = std::fs::read(&exe).unwrap();
    let out = env.run(&exe, &["update", "--yes"]);
    assert!(out.status.success(), "{}", text(&out));
    assert_eq!(env.log("brew").trim(), "upgrade larrykoo711/tap/tmux-haha");
    // ta left the Cellar to Homebrew; nothing was downloaded.
    assert_eq!(std::fs::read(&exe).unwrap(), before);
    assert_eq!(env.log("curl").lines().count(), 1, "{}", env.log("curl"));
    assert!(
        text(&out).contains(&format!("Updated to {NEW}.")),
        "{}",
        text(&out)
    );
}
