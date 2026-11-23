//! `ta update`: look for a new version right now (only when asked) and,
//! once you agree, install it the way this copy was installed: Homebrew,
//! cargo, or a release download that ta replaces itself.
//!
//! A self-replace never touches the running program until the very end: the
//! release is downloaded next to it, checked against its `.sha256`, unpacked,
//! asked for its version, and only then renamed over the old one.
//! Adapted from cc-moss (MIT, https://github.com/larrykoo711/cc-moss), `cmd/update.rs`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};

use crate::command::{run_with_timeout, RunError};

const RELEASES_URL: &str = "https://github.com/larrykoo711/tmux-haha/releases";
const TAP: &str = "larrykoo711/tap/tmux-haha";
const GIT_URL: &str = "https://github.com/larrykoo711/tmux-haha";
const CHECK_TIMEOUT: Duration = Duration::from_secs(15);
const DOWNLOAD_TIMEOUT_SECS: &str = "300";
const SELF_TEST_TIMEOUT: Duration = Duration::from_secs(10);

/// A release version, `major.minor.patch`. Pre-releases don't parse, so they
/// never count as new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(u64, u64, u64);

impl Version {
    pub fn parse(text: &str) -> Option<Version> {
        let text = text.trim();
        let text = text.strip_prefix('v').unwrap_or(text);
        let mut parts = text.split('.');
        let mut next = || -> Option<u64> {
            let p = parts.next()?;
            if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            p.parse().ok()
        };
        let v = Version(next()?, next()?, next()?);
        parts.next().is_none().then_some(v)
    }

    pub fn current() -> Version {
        let v = env!("CARGO_PKG_VERSION");
        Version::parse(v.split('-').next().unwrap_or(v)).unwrap_or(Version(0, 0, 0))
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

/// The release tag in the `location:` header of the `…/releases/latest` redirect.
pub fn latest_from_headers(headers: &str) -> Option<Version> {
    headers.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if !name.trim().eq_ignore_ascii_case("location") {
            return None;
        }
        let (_, tag) = value.trim().trim_end_matches('/').rsplit_once("/tag/")?;
        Version::parse(tag)
    })
}

/// The releases page; `TA_RELEASES_URL` replaces it in debug builds (tests).
fn releases_url() -> String {
    std::env::var("TA_RELEASES_URL")
        .ok()
        .filter(|u| cfg!(debug_assertions) && !u.is_empty())
        .map(|u| u.trim_end_matches('/').to_string())
        .unwrap_or_else(|| RELEASES_URL.to_string())
}

fn fetch_latest() -> Result<Version> {
    let url = format!("{}/latest", releases_url());
    let mut cmd = Command::new("curl");
    cmd.args(["-fsS", "-I", "--proto", "=https", "--tlsv1.2", "--max-time"])
        .arg(CHECK_TIMEOUT.as_secs().to_string())
        .arg(&url);
    let why = |e: String| {
        anyhow!(tr!(
            format!("could not check for a new version ({e}); check your connection and try again"),
            format!("没能检查新版本（{e}），看看网络再试一次")
        ))
    };
    let out = match run_with_timeout(&mut cmd, CHECK_TIMEOUT + Duration::from_secs(1)) {
        Ok(out) => out,
        Err(e) if e.is_not_installed() => return Err(why("curl is not installed".into())),
        Err(RunError::NotStarted(e)) => return Err(why(e.to_string())),
        Err(RunError::TimedOut) => return Err(why("no answer in time".into())),
    };
    if !out.status.success() {
        return Err(why(String::from_utf8_lossy(&out.stderr).trim().to_string()));
    }
    latest_from_headers(&String::from_utf8_lossy(&out.stdout))
        .ok_or_else(|| why("no release found".into()))
}

/// How this copy was installed, and so how to update it.
#[derive(Debug, PartialEq, Eq)]
enum Method {
    Homebrew,
    Cargo,
    /// A release download: replace the file at this path.
    Binary(PathBuf),
}

impl Method {
    fn detect(exe: &Path) -> Method {
        if exe.to_string_lossy().contains("/Cellar/tmux-haha/") {
            Method::Homebrew
        } else if cargo_bin().is_some_and(|bin| exe.parent() == Some(bin.as_path())) {
            Method::Cargo
        } else {
            Method::Binary(exe.to_path_buf())
        }
    }

    fn command(&self, version: &Version) -> Option<Vec<String>> {
        Some(match self {
            Method::Homebrew => vec!["brew".into(), "upgrade".into(), TAP.into()],
            Method::Cargo => ["cargo", "install", "--locked", "--git", GIT_URL, "--tag"]
                .iter()
                .map(|s| s.to_string())
                .chain([format!("v{version}"), "tmux-haha".into()])
                .collect(),
            Method::Binary(_) => return None,
        })
    }

    /// Where the upgraded program can be asked for its version.
    fn installed_path(&self, exe: &Path) -> Option<PathBuf> {
        let path = match self {
            Method::Homebrew => {
                let text = exe.to_string_lossy();
                let (prefix, _) = text.split_once("/Cellar/tmux-haha/")?;
                Path::new(prefix).join("bin").join("ta")
            }
            Method::Cargo => exe.to_path_buf(),
            Method::Binary(_) => return None,
        };
        path.exists().then_some(path)
    }
}

/// `$CARGO_HOME/bin`, else `~/.cargo/bin`, with symlinks resolved.
fn cargo_bin() -> Option<PathBuf> {
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let home = env("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| env("HOME").map(|h| PathBuf::from(h).join(".cargo")))?;
    std::fs::canonicalize(home.join("bin")).ok()
}

/// The running program, symlinks resolved. `TA_UPDATE_EXE` stands in for it
/// in debug builds, so tests update a copy and never the test binary.
fn current_exe() -> Result<PathBuf> {
    let exe = match std::env::var_os("TA_UPDATE_EXE")
        .filter(|v| cfg!(debug_assertions) && !v.is_empty())
    {
        Some(p) => PathBuf::from(p),
        None => std::env::current_exe().context("cannot find the running ta")?,
    };
    std::fs::canonicalize(&exe).with_context(|| format!("cannot resolve {}", exe.display()))
}

/// The release build for this machine, as named in release.yml.
fn target() -> Option<&'static str> {
    Some(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("linux", "x86_64") => "x86_64-unknown-linux-musl",
        ("linux", "aarch64") => "aarch64-unknown-linux-musl",
        _ => return None,
    })
}

pub fn run(check: bool, yes: bool) -> Result<()> {
    let current = Version::current();
    let latest = fetch_latest()?;
    if latest <= current {
        println!(
            "{}",
            tr!(
                format!("You already have the latest ta ({current})."),
                format!("你用的已经是最新的 ta（{current}）。")
            )
        );
        return Ok(());
    }
    let notes = format!("{}/tag/v{latest}", releases_url());
    println!(
        "{}",
        tr!(
            format!("ta {latest} is out (you have {current}). What's new: {notes}"),
            format!("ta {latest} 发布了（你现在是 {current}）。更新了什么：{notes}")
        )
    );
    if check {
        println!(
            "{}",
            tr!("Run `ta update` to get it.", "运行 `ta update` 就能升级。")
        );
        return Ok(());
    }
    let exe = current_exe()?;
    let method = Method::detect(&exe);
    match (&method, method.command(&latest)) {
        (Method::Binary(exe), _) => println!(
            "{}",
            tr!(
                format!(
                    "ta will download {latest}, check it, and put it in place of {}.",
                    exe.display()
                ),
                format!("ta 会下载 {latest}，校验无误后替换 {}。", exe.display())
            )
        ),
        (_, Some(cmd)) => println!(
            "{}",
            tr!(
                format!("This runs: {}", cmd.join(" ")),
                format!("会运行：{}", cmd.join(" "))
            )
        ),
        _ => {}
    }
    let question = tr!(
        format!("Update to {latest} now?"),
        format!("现在升级到 {latest} 吗？")
    );
    let refused = tr!(
        "add --yes to update without asking",
        "加上 --yes 就不用问了"
    );
    if !crate::confirm::ask(yes, &question, refused, true)? {
        println!("{}", tr!("Okay, maybe later.", "好，下次再说。"));
        return Ok(());
    }
    match &method {
        Method::Binary(exe) => replace_self(exe, &latest)?,
        managed => {
            run_manager(&managed.command(&latest).expect("a package manager"))?;
            confirm_upgraded(managed, &exe, &latest)?;
        }
    }
    println!(
        "{}",
        tr!(
            format!("Updated to {latest}."),
            format!("已升级到 {latest}。")
        )
    );
    Ok(())
}

fn run_manager(cmd: &[String]) -> Result<()> {
    let line = cmd.join(" ");
    match Command::new(&cmd[0]).args(&cmd[1..]).status() {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => bail!(tr!(
            format!("`{line}` did not succeed ({s}); its output above says why"),
            format!("`{line}` 没有成功（{s}），原因见上面的输出")
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => bail!(tr!(
            format!("{} is not on PATH; run `{line}` where it is", cmd[0]),
            format!("PATH 里找不到 {}；请在能找到它的地方运行 `{line}`", cmd[0])
        )),
        Err(e) => bail!(tr!(
            format!("could not run `{line}`: {e}"),
            format!("没能运行 `{line}`：{e}")
        )),
    }
}

/// A package manager can finish happily without upgrading (a tap that has
/// not caught up yet): ask the installed program.
fn confirm_upgraded(method: &Method, exe: &Path, latest: &Version) -> Result<()> {
    let Some(installed) = method.installed_path(exe) else {
        return Ok(());
    };
    let said = version_of(&installed);
    if said == format!("ta {latest}") {
        return Ok(());
    }
    bail!(tr!(
        format!("the package manager finished, but {} says `{said}`, not ta {latest}; try again in a while (with Homebrew: `brew update` first)", installed.display()),
        format!("包管理器跑完了，但 {} 说的是 `{said}`，不是 ta {latest}；过一会儿再试（Homebrew 可以先 `brew update`）", installed.display())
    ))
}

fn version_of(program: &Path) -> String {
    run_with_timeout(Command::new(program).arg("--version"), SELF_TEST_TIMEOUT)
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

/// Removes the work folder however the update ends.
struct WorkDir(PathBuf);

impl Drop for WorkDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Download, verify, test and swap in the release for this machine.
fn replace_self(exe: &Path, version: &Version) -> Result<()> {
    let Some(target) = target() else {
        bail!(tr!(
            format!(
                "there is no release build for this system; get {version} from {}",
                releases_url()
            ),
            format!(
                "这个系统没有现成的发布包；请到 {} 获取 {version}",
                releases_url()
            )
        ));
    };
    let name = format!("tmux-haha-v{version}-{target}");
    let tarball = format!("{name}.tar.gz");
    let url = format!("{}/download/v{version}/{tarball}", releases_url());
    let dir = exe.parent().context("the program has no folder")?;
    // Work next to the program, so the final rename stays on one filesystem.
    let work = dir.join(format!(".ta-update-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    if let Err(e) = std::fs::create_dir(&work) {
        if matches!(
            e.kind(),
            std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::ReadOnlyFilesystem
        ) {
            let steps = format!(
                "  curl -fsSLO {url}\n  tar -xzf {tarball}\n  sudo install -m 755 {name}/ta {}",
                exe.display()
            );
            bail!(tr!(
                format!("cannot write to {}, so ta cannot replace itself; nothing was changed. To update by hand:\n{steps}", dir.display()),
                format!("没有 {} 的写权限，ta 没法替换自己；什么都没改。可以手动升级：\n{steps}", dir.display())
            ));
        }
        return Err(unchanged(anyhow!(e)));
    }
    let work = WorkDir(work);
    swap_in(exe, version, &work.0, &url, &tarball, &name).map_err(unchanged)
}

fn swap_in(
    exe: &Path,
    version: &Version,
    work: &Path,
    url: &str,
    tarball: &str,
    name: &str,
) -> Result<()> {
    println!(
        "{}",
        tr!(
            format!("Downloading {tarball} …"),
            format!("正在下载 {tarball}……")
        )
    );
    let archive = work.join(tarball);
    let sums = work.join(format!("{tarball}.sha256"));
    download(url, &archive)?;
    download(&format!("{url}.sha256"), &sums)?;
    verify(&archive, &sums)?;
    // The archive carries the build machine's uid; never restore it.
    let out = Command::new("tar")
        .args(["--no-same-owner", "--no-same-permissions", "-xzf"])
        .arg(&archive)
        .arg("-C")
        .arg(work)
        .output()
        .map_err(|e| anyhow!("tar: {e}"))?;
    let new = work.join(name).join("ta");
    // A plain file, not a link to something else: checked before anything runs.
    let plain_file = std::fs::symlink_metadata(&new).is_ok_and(|m| m.file_type().is_file());
    if !out.status.success() || !plain_file {
        bail!(tr!(
            format!(
                "the download did not unpack into {name}/ta: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ),
            format!(
                "下载的包没能解出 {name}/ta：{}",
                String::from_utf8_lossy(&out.stderr).trim()
            )
        ));
    }
    let old = std::fs::metadata(exe)?;
    std::fs::set_permissions(&new, old.permissions())?;
    keep_owner(&new, &old)?;
    let said = version_of(&new);
    if said != format!("ta {version}") {
        bail!(tr!(
            format!("the downloaded ta says `{said}`, not ta {version}"),
            format!("下载的 ta 说的是 `{said}`，不是 ta {version}")
        ));
    }
    std::fs::rename(&new, exe).map_err(|e| {
        anyhow!(tr!(
            format!("could not put the new version in place: {e}"),
            format!("没能把新版本放到位：{e}")
        ))
    })
}

/// The new file belongs to whoever owned the old one.
fn keep_owner(new: &Path, old: &std::fs::Metadata) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    if std::os::unix::fs::chown(new, Some(old.uid()), Some(old.gid())).is_ok() {
        return Ok(());
    }
    if std::fs::metadata(new)?.uid() != old.uid() {
        bail!(tr!(
            format!(
                "the new file would belong to a different user than {}",
                new.display()
            ),
            format!("新文件的所有者会和原来的 {} 不一样", new.display())
        ));
    }
    Ok(())
}

fn unchanged(e: anyhow::Error) -> anyhow::Error {
    anyhow!(tr!(
        format!("the update did not go through, so your ta is unchanged: {e:#}"),
        format!("这次没升级成功，你的 ta 原样没动：{e:#}")
    ))
}

fn download(url: &str, to: &Path) -> Result<()> {
    let out = Command::new("curl")
        .args([
            "-fsSL",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--tlsv1.2",
            "--max-time",
            DOWNLOAD_TIMEOUT_SECS,
            "-o",
        ])
        .arg(to)
        .arg(url)
        .output();
    match out {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => bail!(tr!(
            format!(
                "could not download {url}: {}",
                String::from_utf8_lossy(&o.stderr).trim()
            ),
            format!(
                "没能下载 {url}：{}",
                String::from_utf8_lossy(&o.stderr).trim()
            )
        )),
        Err(e) => bail!(tr!(
            format!("could not download {url}: curl: {e}"),
            format!("没能下载 {url}：curl：{e}")
        )),
    }
}

/// The download's SHA-256 against the first word of its `.sha256` file.
fn verify(archive: &Path, sums: &Path) -> Result<()> {
    let expected = std::fs::read_to_string(sums)?
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let actual = sha256(archive)?;
    if expected.len() != 64 || expected != actual {
        bail!(tr!(
            "the download does not match its published checksum",
            "下载的文件和发布的校验和对不上"
        ));
    }
    println!("{}", tr!("Checksum matches.", "校验和一致。"));
    Ok(())
}

/// SHA-256 with the system's tool: `sha256sum` (Linux), else `shasum`.
fn sha256(path: &Path) -> Result<String> {
    let tools: [(&str, &[&str]); 2] = [("sha256sum", &[]), ("shasum", &["-a", "256"])];
    for (tool, args) in tools {
        if let Ok(out) = Command::new(tool).args(args).arg(path).output() {
            if out.status.success() {
                if let Some(hex) = String::from_utf8_lossy(&out.stdout)
                    .split_whitespace()
                    .next()
                {
                    return Ok(hex.to_ascii_lowercase());
                }
            }
        }
    }
    bail!(tr!(
        "cannot verify the download: neither sha256sum nor shasum is installed",
        "没法校验下载的文件：sha256sum 和 shasum 都没装"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_and_skip_prereleases() {
        assert!(Version::parse("v0.2.0").unwrap() > Version::parse("0.1.9").unwrap());
        assert!(Version::parse("0.10.0").unwrap() > Version::parse("0.9.0").unwrap());
        assert_eq!(Version::parse("0.2.0-rc.1"), None);
        assert_eq!(Version::parse("0.2"), None);
    }

    #[test]
    fn latest_comes_from_the_redirect() {
        let headers = "HTTP/2 302\r\nLocation: https://github.com/larrykoo711/tmux-haha/releases/tag/v0.3.1\r\n";
        assert_eq!(latest_from_headers(headers), Version::parse("0.3.1"));
        assert_eq!(latest_from_headers("HTTP/2 404\r\n"), None);
    }

    #[test]
    fn install_method_from_the_path() {
        assert_eq!(
            Method::detect(Path::new("/opt/homebrew/Cellar/tmux-haha/0.1.0/bin/ta")),
            Method::Homebrew
        );
        let v = Version::parse("0.2.0").unwrap();
        assert_eq!(
            Method::Homebrew.command(&v).unwrap().join(" "),
            "brew upgrade larrykoo711/tap/tmux-haha"
        );
        assert!(Method::Cargo
            .command(&v)
            .unwrap()
            .join(" ")
            .ends_with("--tag v0.2.0 tmux-haha"));
        assert!(matches!(
            Method::detect(Path::new("/usr/local/bin/ta")),
            Method::Binary(_)
        ));
    }
}
