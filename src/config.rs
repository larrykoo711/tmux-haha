//! `~/.config/ta/config.toml`. No file means defaults; a broken file is an
//! error that names the file, never a silent fallback.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use serde::Deserialize;

#[derive(Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    /// The tmux server: a socket name (`tmux -L`) or path (`tmux -S`).
    pub socket: Option<String>,
    /// Name new sessions `<host>_<name>`.
    pub host_prefix: bool,
    /// `auto`, `en` or `zh`.
    pub language: Option<String>,
    pub folders: Folders,
}

#[derive(Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, default)]
pub struct Folders {
    /// Folders whose subfolders the folder tab offers; `~/` is the home folder.
    pub roots: Vec<String>,
}

/// Where the file lives: `$XDG_CONFIG_HOME/ta/config.toml`, else
/// `~/.config/ta/config.toml`.
pub fn path(env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let base = env("XDG_CONFIG_HOME")
        .filter(|v| Path::new(v).is_absolute())
        .map(PathBuf::from)
        .or_else(|| {
            env("HOME")
                .filter(|h| !h.is_empty())
                .map(|h| Path::new(&h).join(".config"))
        })?;
    Some(base.join("ta").join("config.toml"))
}

/// The config for this process.
pub fn load() -> Result<Config> {
    let Some(path) = path(&|k| std::env::var(k).ok()) else {
        return Ok(Config::default());
    };
    match std::fs::read_to_string(&path) {
        Ok(text) => parse(&text, &path),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
        Err(e) => Err(anyhow!("{}: {e}", path.display())),
    }
}

pub fn parse(text: &str, path: &Path) -> Result<Config> {
    toml::from_str(text).map_err(|e| {
        anyhow!(
            "{}: {}",
            path.display(),
            e.to_string().trim_end().replace('\n', "\n  ")
        )
    })
}

impl Config {
    /// The folder roots with `~/` expanded.
    // Read by the folder tab (013 T3.2), which lands next.
    #[allow(dead_code)]
    pub fn roots(&self, home: &str) -> Vec<String> {
        self.folders
            .roots
            .iter()
            .map(|r| match r.strip_prefix("~/") {
                Some(rest) if !home.is_empty() => format!("{}/{rest}", home.trim_end_matches('/')),
                _ if r == "~" && !home.is_empty() => home.to_string(),
                _ => r.clone(),
            })
            .collect()
    }
}

/// The tmux server to use: the flag or `TA_TMUX_SOCKET` (`cli`), else the config.
pub fn socket(cli: Option<&str>, config: &Config) -> Option<String> {
    cli.filter(|v| !v.is_empty())
        .map(String::from)
        .or_else(|| config.socket.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p() -> PathBuf {
        PathBuf::from("/h/.config/ta/config.toml")
    }

    #[test]
    fn empty_is_default() {
        assert_eq!(parse("", &p()).unwrap(), Config::default());
    }

    #[test]
    fn every_field() {
        let c = parse(
            "socket = \"work\"\nhost_prefix = true\nlanguage = \"zh\"\n[folders]\nroots = [\"~/code\", \"/srv\"]\n",
            &p(),
        )
        .unwrap();
        assert_eq!(c.socket.as_deref(), Some("work"));
        assert!(c.host_prefix);
        assert_eq!(c.language.as_deref(), Some("zh"));
        assert_eq!(c.folders.roots, ["~/code", "/srv"]);
    }

    #[test]
    fn tilde_roots() {
        let c = parse(
            "[folders]\nroots = [\"~/code\", \"~\", \"/srv\", \"~x\"]",
            &p(),
        )
        .unwrap();
        assert_eq!(c.roots("/h/me"), ["/h/me/code", "/h/me", "/srv", "~x"]);
    }

    #[test]
    fn a_broken_config_says_where_and_what() {
        for (text, what) in [
            ("sockett = \"x\"", "unknown field"),
            ("host_prefix = \"yes\"", "invalid type"),
            ("socket = ", "string values must be quoted"),
        ] {
            let e = parse(text, &p()).unwrap_err().to_string();
            assert!(e.starts_with("/h/.config/ta/config.toml: "), "{e}");
            assert!(e.contains(what) || e.contains("expected"), "{text}: {e}");
        }
    }

    #[test]
    fn xdg_wins() {
        let env = |k: &str| match k {
            "XDG_CONFIG_HOME" => Some("/x".to_string()),
            "HOME" => Some("/h".to_string()),
            _ => None,
        };
        assert_eq!(path(&env), Some(PathBuf::from("/x/ta/config.toml")));
        let env = |k: &str| (k == "HOME").then(|| "/h".to_string());
        assert_eq!(path(&env), Some(PathBuf::from("/h/.config/ta/config.toml")));
        // A relative XDG_CONFIG_HOME is ignored, as the spec says.
        let env = |k: &str| match k {
            "XDG_CONFIG_HOME" => Some("rel".to_string()),
            "HOME" => Some("/h".to_string()),
            _ => None,
        };
        assert_eq!(path(&env), Some(PathBuf::from("/h/.config/ta/config.toml")));
    }

    #[test]
    fn flag_and_env_beat_the_config() {
        let c = Config {
            socket: Some("from-config".into()),
            ..Config::default()
        };
        assert_eq!(socket(Some("from-flag"), &c).as_deref(), Some("from-flag"));
        assert_eq!(socket(None, &c).as_deref(), Some("from-config"));
        assert_eq!(socket(Some(""), &c).as_deref(), Some("from-config"));
        assert_eq!(socket(None, &Config::default()), None);
    }
}
