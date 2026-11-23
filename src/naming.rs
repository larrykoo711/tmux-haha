//! tmux session names. Adapted from cc-moss (MIT,
//! https://github.com/larrykoo711/cc-moss), `moss-core/src/naming.rs`.

use std::process::Command;
use std::time::Duration;

use crate::command::output_with_timeout;

/// The longest tmux name part, in characters.
pub const MAX_CHARS: usize = 40;

/// A session name as a tmux name: what people name things with stays (any
/// script, case, emoji); only what tmux or a shell can't take becomes `-`:
/// white space, `.` and `:` (tmux's target separators), and characters a
/// shell would read (`'"$\`!#;&|<>(){}[]*?~^,=/`). Control characters go;
/// so do a leading `-` and the `$` `@` `%` that tmux reads as ids. Runs of
/// `-` collapse, at most [`MAX_CHARS`] characters. Never empty.
/// (notes/session-names.md, 2026-10-19: Chinese names used to vanish.)
pub fn sanitize(name: &str) -> String {
    const SHELL: &str = ".:'\"$`\\!#;&|<>(){}[]*?~^,=/";
    let mut out = String::with_capacity(name.len());
    let chars: Vec<char> = name.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_control() && !c.is_whitespace() {
            continue;
        }
        if hidden_at(&chars, i) {
            continue;
        }
        let c = if c.is_whitespace() || SHELL.contains(c) {
            '-'
        } else {
            c
        };
        if c == '-' && (out.is_empty() || out.ends_with('-')) {
            continue;
        }
        if out.is_empty() && matches!(c, '@' | '%') {
            continue;
        }
        out.push(c);
    }
    let cut: String = out.chars().take(MAX_CHARS).collect();
    let trimmed = cut.trim_end_matches('-');
    if trimmed.is_empty() {
        "session".to_string()
    } else {
        trimmed.to_string()
    }
}

fn hidden_at(chars: &[char], i: usize) -> bool {
    let c = chars[i];
    if c == '\u{200D}' {
        let before = i.checked_sub(1).map(|j| chars[j]);
        let after = chars.get(i + 1).copied();
        return !(before.is_some_and(emoji_part) && after.is_some_and(emoji_part));
    }
    format_or_separator(c)
}

/// Emoji (skin tones among them), and the variation selector that ends one.
fn emoji_part(c: char) -> bool {
    matches!(c as u32,
        0x1F000..=0x1FAFF | 0x2600..=0x27BF | 0x2B00..=0x2BFF | 0xFE0F)
}

/// Unicode categories Cf, Zl and Zp.
fn format_or_separator(c: char) -> bool {
    matches!(c as u32,
        0x00AD
        | 0x0600..=0x0605
        | 0x061C
        | 0x06DD
        | 0x070F
        | 0x0890..=0x0891
        | 0x08E2
        | 0x180E
        | 0x200B..=0x200F
        | 0x2028..=0x202E
        | 0x2060..=0x2064
        | 0x2066..=0x206F
        | 0xFEFF
        | 0xFFF9..=0xFFFB
        | 0x110BD
        | 0x110CD
        | 0x13430..=0x1343F
        | 0x1BCA0..=0x1BCA3
        | 0x1D173..=0x1D17A
        | 0xE0001
        | 0xE0020..=0xE007F)
}

/// `<prefix>_<name>`, or just the name without a prefix. The machine's
/// prefix stays lowercase, as it always was.
pub fn session_name(prefix: Option<&str>, name: &str) -> String {
    match prefix {
        Some(p) => format!("{}_{}", sanitize(p).to_lowercase(), sanitize(name)),
        None => sanitize(name),
    }
}

/// What ta calls a session it starts: the name asked for, else the folder's
/// name; with this machine's prefix when there is one.
pub fn new_session_name(asked: Option<&str>, folder: &str, host: Option<&str>) -> String {
    match asked.filter(|a| !a.trim().is_empty()) {
        Some(a) => session_name(host, a),
        None => session_name(host, &short_name(folder)),
    }
}

/// `desired`, or `desired-2`, `desired-3`… if another session already has it.
/// `current` is the session being renamed; its own name is not a conflict.
pub fn unique(desired: &str, taken: &[String], current: Option<&str>) -> String {
    let is_taken = |n: &str| current != Some(n) && taken.iter().any(|t| t == n);
    if !is_taken(desired) {
        return desired.to_string();
    }
    (2..)
        .map(|i| format!("{desired}-{i}"))
        .find(|n| !is_taken(n))
        .unwrap_or_else(|| desired.to_string())
}

/// The machine's short name: `$TA_HOST_PREFIX` if set, else
/// `LocalHostName` on macOS (`hostname -s` often returns a generic name
/// there), else the kernel's host name on Linux, else `hostname -s`.
pub fn host_prefix() -> Option<String> {
    if let Some(p) = std::env::var("TA_HOST_PREFIX")
        .ok()
        .filter(|v| !v.is_empty())
    {
        return Some(p);
    }
    let run = |cmd: &mut Command| {
        output_with_timeout(cmd, Duration::from_millis(500))
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty())
    };
    if cfg!(target_os = "macos") {
        if let Some(name) = run(Command::new("scutil").args(["--get", "LocalHostName"])) {
            return Some(name);
        }
    }
    if cfg!(target_os = "linux") {
        if let Ok(name) = std::fs::read_to_string("/proc/sys/kernel/hostname") {
            let short = name.trim().split('.').next().unwrap_or_default();
            if !short.is_empty() {
                return Some(short.to_string());
            }
        }
    }
    run(Command::new("hostname").arg("-s"))
}

/// The longest standard session name, in characters.
pub const STANDARD_CHARS: usize = 24;

/// `name` cut to [`STANDARD_CHARS`] characters, at a `-` when there is one
/// to cut at, with no `-` left at the end.
fn cut(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    if chars.len() <= STANDARD_CHARS {
        return name.to_string();
    }
    let mut kept: String = chars[..STANDARD_CHARS].iter().collect();
    // Cut mid-word: back to the last `-`, if any.
    if chars[STANDARD_CHARS] != '-' {
        if let Some(at) = kept.rfind('-').filter(|at| *at > 0) {
            kept.truncate(at);
        }
    }
    kept.trim_end_matches('-').to_string()
}

/// A short name for a session from its folder: the folder's own name, made
/// standard. The caller makes it unique ([`unique`]).
pub fn short_name(folder: &str) -> String {
    let last = folder
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("");
    cut(&sanitize(last))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_keeps_what_people_name_things_with() {
        assert_eq!(sanitize("接口评审"), "接口评审");
        assert_eq!(sanitize("emoji🚀"), "emoji🚀");
        assert_eq!(sanitize("API Review v1.2"), "API-Review-v1-2");
        assert_eq!(sanitize(" fix: login "), "fix-login");
        assert_eq!(sanitize("$x"), "x");
        assert_eq!(sanitize(""), "session");
        assert_eq!(sanitize("a\u{202E}b"), "ab");
    }

    #[test]
    fn unique_adds_a_number() {
        let taken = vec!["api".to_string(), "api-2".to_string()];
        assert_eq!(unique("web", &taken, None), "web");
        assert_eq!(unique("api", &taken, None), "api-3");
        assert_eq!(unique("api", &taken, Some("api")), "api");
    }

    #[test]
    fn host_prefix_comes_first() {
        assert_eq!(session_name(Some("Work-Laptop"), "api"), "work-laptop_api");
        assert_eq!(session_name(Some("box"), "my notes"), "box_my-notes");
        assert_eq!(session_name(None, "api"), "api");
    }

    #[test]
    fn new_sessions_are_named_from_the_ask_or_the_folder() {
        assert_eq!(
            new_session_name(Some("my notes"), "/w/shop", None),
            "my-notes"
        );
        assert_eq!(new_session_name(None, "/w/shop", None), "shop");
        assert_eq!(
            new_session_name(Some(" "), "/w/shop", Some("box")),
            "box_shop"
        );
        assert_eq!(
            new_session_name(Some("api"), "/w/shop", Some("box")),
            "box_api"
        );
    }

    #[test]
    fn short_names_come_from_the_folder() {
        assert_eq!(short_name("/Users/me/work/webshop"), "webshop");
        assert_eq!(short_name("/Users/me/work/webshop/"), "webshop");
        assert_eq!(short_name("/Users/me/项目 资料"), "项目-资料");
        assert_eq!(short_name("/"), "session");
        assert!(
            short_name("/w/an-extremely-long-folder-name-for-a-project")
                .chars()
                .count()
                <= STANDARD_CHARS
        );
    }
}
