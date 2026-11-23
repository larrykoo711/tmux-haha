//! Folders for the picker's folder tab: the ones zoxide knows (most used
//! first), then the folders inside the configured roots.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use crate::command::run_with_timeout;

/// How long zoxide may take before the tab goes without it.
const ZOXIDE_TIMEOUT: Duration = Duration::from_secs(1);
/// At most this many of zoxide's folders.
const ZOXIDE_MAX: usize = 200;

/// zoxide's folders, most used first; none when it is missing, fails or hangs.
pub fn zoxide() -> Vec<String> {
    from_command(Command::new("zoxide").args(["query", "-l"]), ZOXIDE_TIMEOUT)
}

fn from_command(cmd: &mut Command, timeout: Duration) -> Vec<String> {
    match run_with_timeout(cmd, timeout) {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter(|l| !l.is_empty())
            .take(ZOXIDE_MAX)
            .map(String::from)
            .collect(),
        _ => Vec::new(),
    }
}

/// The folders directly inside `root`, by name; hidden ones left out.
fn subfolders(root: &str) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut found: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .filter(|e| e.path().is_dir())
        .map(|e| e.path().to_string_lossy().into_owned())
        .collect();
    found.sort();
    found
}

/// zoxide's folders, then the roots' subfolders: each once, only those that exist.
pub fn candidates(zoxide: Vec<String>, roots: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    zoxide
        .into_iter()
        .chain(roots.iter().flat_map(|r| subfolders(r)))
        .map(|p| {
            let trimmed = p.trim_end_matches('/');
            if trimmed.is_empty() {
                p
            } else {
                trimmed.to_string()
            }
        })
        .filter(|p| Path::new(p).is_dir())
        .filter(|p| seen.insert(p.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn folder_candidates_merge_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("code");
        for name in ["beta", "alpha", ".hidden"] {
            std::fs::create_dir_all(root.join(name)).unwrap();
        }
        std::fs::write(root.join("file.txt"), "").unwrap();
        let root = root.to_string_lossy().into_owned();
        let beta = format!("{root}/beta");
        let alpha = format!("{root}/alpha");
        let got = candidates(
            vec![format!("{beta}/"), "/no/such/folder".into()],
            std::slice::from_ref(&root),
        );
        // zoxide's first, then the root's by name; each once.
        assert_eq!(got, [beta, alpha]);
    }

    #[test]
    fn hidden_and_missing_are_left_out() {
        assert!(candidates(vec![], &["/no/such/root".into()]).is_empty());
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        assert!(subfolders(&dir.path().to_string_lossy()).is_empty());
    }

    #[test]
    fn zoxide_that_hangs_is_skipped() {
        let start = Instant::now();
        let got = from_command(
            Command::new("sh").args(["-c", "sleep 5"]),
            Duration::from_millis(200),
        );
        assert!(got.is_empty());
        assert!(start.elapsed() < Duration::from_secs(2));
        // A failing or missing one: nothing either.
        assert!(from_command(&mut Command::new("false"), ZOXIDE_TIMEOUT).is_empty());
        assert!(from_command(&mut Command::new("no-such-zoxide"), ZOXIDE_TIMEOUT).is_empty());
        // A working one: its lines.
        let got = from_command(Command::new("printf").arg("/a\\n/b\\n"), ZOXIDE_TIMEOUT);
        assert_eq!(got, ["/a", "/b"]);
    }
}
