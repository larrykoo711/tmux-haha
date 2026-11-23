//! `ta config`: where the config file is, and each setting's value and
//! where that value comes from.

use anyhow::Result;
use unicode_width::UnicodeWidthStr;

use crate::config::Config;
use crate::doctor::tilde;

/// What a setting's value comes from.
fn from_file() -> &'static str {
    tr!("config file", "配置文件")
}

fn by_default() -> &'static str {
    tr!("default", "默认")
}

/// Print the path, then each setting; with `path_only`, just the path.
pub fn show(cli_socket: Option<&str>, config: &Result<Config>, path_only: bool) -> Result<bool> {
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let path = crate::config::path(&env).unwrap_or_default();
    if path_only {
        println!("{}", path.display());
        return Ok(true);
    }
    let home = env("HOME").unwrap_or_default();
    let shown = tilde(&path.to_string_lossy(), &home);
    let config = match config {
        Ok(c) => {
            if path.exists() {
                println!("{} {shown}", tr!("Config file:", "配置文件："));
            } else {
                println!(
                    "{} {shown}{}",
                    tr!("Config file:", "配置文件："),
                    tr!(" (not there; defaults apply)", "（不存在，用默认值）")
                );
            }
            c
        }
        Err(e) => {
            println!("{} {shown}", tr!("Config file:", "配置文件："));
            println!("  ✗ {e:#}");
            return Ok(false);
        }
    };
    println!();
    let socket = match (cli_socket.filter(|s| !s.is_empty()), env("TA_TMUX_SOCKET")) {
        (Some(s), Some(e)) if s == e => (s.to_string(), "TA_TMUX_SOCKET".to_string()),
        (Some(s), _) => (s.to_string(), "--socket".to_string()),
        (None, _) => match &config.socket {
            Some(s) => (s.clone(), from_file().to_string()),
            None => (
                tr!("tmux's own", "tmux 自己的").to_string(),
                by_default().to_string(),
            ),
        },
    };
    let host = (
        config.host_prefix.to_string(),
        if config.host_prefix {
            from_file()
        } else {
            by_default()
        }
        .to_string(),
    );
    let lang = match crate::i18n::lang() {
        crate::i18n::Lang::En => "en",
        crate::i18n::Lang::Zh => "zh",
    };
    let language = (
        lang.to_string(),
        if env("TA_LANG").is_some() {
            "TA_LANG".to_string()
        } else if config.language.is_some() {
            from_file().to_string()
        } else {
            tr!("system", "系统语言").to_string()
        },
    );
    let roots = (
        format!("{:?}", config.folders.roots),
        if config.folders.roots.is_empty() {
            by_default()
        } else {
            from_file()
        }
        .to_string(),
    );
    let rows = [
        ("socket", socket),
        ("host_prefix", host),
        ("language", language),
        ("folders.roots", roots),
    ];
    let key_w = rows.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    let val_w = rows.iter().map(|(_, (v, _))| v.width()).max().unwrap_or(0);
    for (key, (value, source)) in rows {
        println!(
            "  {key:<key_w$}  {value}{}  {source}",
            " ".repeat(val_w - value.width())
        );
    }
    Ok(true)
}
