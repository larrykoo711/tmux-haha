//! Identifying what runs inside a tmux pane and styling top popular apps
//! with distinct preset colours, plus reading busy/idle status from pane titles.

use ratatui::style::{Color, Modifier, Style};

/// Visual category of the foreground app running in a pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppTier {
    /// AI coding agents (bold, vivid brand tint).
    Ai,
    /// Top popular developer / terminal apps (distinct brand tint).
    Tool,
    /// Interactive shells (quiet, fades behind real apps).
    Shell,
    /// Any other foreground command.
    Other,
}

/// Normalized app badge and its preset colour style.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppBadge {
    pub label: String,
    pub color: Color,
    pub tier: AppTier,
}

impl AppBadge {
    /// Style for the list row and detail card. On the dark selection band,
    /// quiet shells step up slightly so they stay legible.
    pub fn style(&self, selected: bool) -> Style {
        let fg = match (self.tier, selected) {
            (AppTier::Shell, true) => Color::Rgb(165, 165, 172),
            _ => self.color,
        };
        let mut s = Style::new().fg(fg);
        if self.tier == AppTier::Ai {
            s = s.add_modifier(Modifier::BOLD);
        }
        s
    }
}

/// Live activity inferred from a pane's title (no tool-specific coupling).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activity {
    /// Title starts with a braille spinner (e.g. Claude Code working).
    Busy,
    /// Title starts with `✳` (idle / waiting for input).
    Idle,
    /// Ordinary shell or app without a status prefix.
    Plain,
}

/// Braille spinner frames shown for [`Activity::Busy`].
pub const SPINNER_FRAMES: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// Current spinner frame for a monotonic tick counter.
pub fn spinner_frame(tick: usize) -> char {
    SPINNER_FRAMES[tick % SPINNER_FRAMES.len()]
}

// Preset brand colours for top popular terminal apps and AI coding agents.
// Tuned for contrast on dark terminals, light terminals and the selection band.
const CLAUDE_CORAL: Color = Color::Rgb(232, 131, 92);
const AGY_SKY: Color = Color::Rgb(126, 179, 255);
const CODEX_MINT: Color = Color::Rgb(92, 214, 168);
const GEMINI_VIOLET: Color = Color::Rgb(176, 146, 255);
const OPENCODE_GOLD: Color = Color::Rgb(245, 194, 107);
const AIDER_SAGE: Color = Color::Rgb(138, 226, 114);
const AI_CYAN: Color = Color::Rgb(124, 206, 240);

const EDITOR_GREEN: Color = Color::Rgb(118, 212, 128);
const REMOTE_ROSE: Color = Color::Rgb(235, 118, 132);
const INFRA_BLUE: Color = Color::Rgb(86, 186, 236);
const GIT_ORANGE: Color = Color::Rgb(238, 152, 96);
const PYTHON_GOLD: Color = Color::Rgb(236, 204, 98);
const NODE_LIME: Color = Color::Rgb(156, 212, 112);
const RUST_COPPER: Color = Color::Rgb(228, 124, 92);
const GO_CYAN: Color = Color::Rgb(98, 206, 224);
const DB_PURPLE: Color = Color::Rgb(192, 156, 232);
const MONITOR_TEAL: Color = Color::Rgb(112, 206, 198);

const SHELL_FAINT: Color = Color::Rgb(116, 116, 122);
const OTHER_GREY: Color = Color::Rgb(162, 162, 168);

/// True for Claude Code's native binary process name, which is its semver
/// version string (e.g. `2.1.330`).
fn is_semver_command(cmd: &str) -> bool {
    let mut parts = cmd.split('.');
    let mut count = 0;
    for part in &mut parts {
        if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
        count += 1;
    }
    count >= 3
}

/// Normalize a raw command name (strip path prefix and login-shell `-`).
fn base_command(raw: &str) -> &str {
    let trimmed = raw.trim();
    let base = trimmed.rsplit('/').next().unwrap_or(trimmed);
    base.strip_prefix('-').unwrap_or(base)
}

/// Detect the app badge and preset colour style from tmux's
/// `pane_current_command` and `pane_title`.
pub fn detect(command: &str, title: &str) -> AppBadge {
    let base = base_command(command);
    let lower = base.to_ascii_lowercase();
    let title_lower = title.trim().to_ascii_lowercase();

    // 1. AI coding agents (top priority, vivid distinct styles).
    if lower == "claude"
        || is_semver_command(base)
        || (matches!(lower.as_str(), "node" | "bun")
            && (activity(title) != Activity::Plain || title_lower.contains("claude")))
    {
        return AppBadge {
            label: "claude".into(),
            color: CLAUDE_CORAL,
            tier: AppTier::Ai,
        };
    }
    if matches!(lower.as_str(), "antigravity" | "agy" | "agyx")
        || title_lower.starts_with("agy ")
        || title_lower == "agy"
    {
        return AppBadge {
            label: "agy".into(),
            color: AGY_SKY,
            tier: AppTier::Ai,
        };
    }
    if lower == "codex" || (lower == "node" && title_lower.contains("codex")) {
        return AppBadge {
            label: "codex".into(),
            color: CODEX_MINT,
            tier: AppTier::Ai,
        };
    }
    if lower == "gemini" || (lower == "node" && title_lower.contains("gemini")) {
        return AppBadge {
            label: "gemini".into(),
            color: GEMINI_VIOLET,
            tier: AppTier::Ai,
        };
    }
    if lower == "opencode" {
        return AppBadge {
            label: "opencode".into(),
            color: OPENCODE_GOLD,
            tier: AppTier::Ai,
        };
    }
    if lower == "aider" {
        return AppBadge {
            label: "aider".into(),
            color: AIDER_SAGE,
            tier: AppTier::Ai,
        };
    }
    if matches!(
        lower.as_str(),
        "copilot" | "kiro" | "goose" | "amp" | "cline" | "droid" | "cursor-agent"
    ) {
        let label = if lower == "cursor-agent" {
            "cursor"
        } else {
            lower.as_str()
        };
        return AppBadge {
            label: label.into(),
            color: AI_CYAN,
            tier: AppTier::Ai,
        };
    }

    // 2. Top popular developer & terminal apps.
    match lower.as_str() {
        "nvim" | "neovim" => return tool("nvim", EDITOR_GREEN),
        "vim" | "vi" | "hx" | "helix" | "emacs" | "nano" | "micro" => {
            let label = match lower.as_str() {
                "helix" => "hx",
                other => other,
            };
            return tool(label, EDITOR_GREEN);
        }
        "ssh" | "mosh" | "mosh-client" | "sftp" => {
            let label = if lower == "mosh-client" {
                "mosh"
            } else {
                lower.as_str()
            };
            return tool(label, REMOTE_ROSE);
        }
        "docker" | "lazydocker" | "podman" | "k9s" | "kubectl" | "helm" | "terraform" => {
            return tool(&lower, INFRA_BLUE);
        }
        "lazygit" | "git" | "tig" | "gitui" | "jj" | "gh" => {
            return tool(&lower, GIT_ORANGE);
        }
        "python" | "python3" | "ipython" | "uv" | "pytest" => {
            let label = if lower.starts_with("python") {
                "python"
            } else {
                lower.as_str()
            };
            return tool(label, PYTHON_GOLD);
        }
        s if s.starts_with("python3.") => return tool("python", PYTHON_GOLD),
        "node" | "bun" | "deno" | "pnpm" | "npm" | "yarn" | "tsx" | "vite" => {
            return tool(&lower, NODE_LIME);
        }
        "cargo" | "rustc" | "rustup" => return tool(&lower, RUST_COPPER),
        "go" | "dlv" => return tool(&lower, GO_CYAN),
        "psql" | "pgcli" | "sqlite3" | "mysql" | "mycli" | "redis-cli" | "mongosh" => {
            return tool(&lower, DB_PURPLE);
        }
        "btop" | "htop" | "top" | "nvtop" | "yazi" | "ranger" | "lf" | "mc" => {
            return tool(&lower, MONITOR_TEAL);
        }
        _ => {}
    }

    // 3. Interactive shells (quiet neutral styling).
    if matches!(
        lower.as_str(),
        "zsh" | "bash" | "fish" | "sh" | "dash" | "ash" | "nu" | "tcsh" | "csh" | "ksh" | "pwsh"
    ) {
        return AppBadge {
            label: lower,
            color: SHELL_FAINT,
            tier: AppTier::Shell,
        };
    }

    // 4. Fallback: show whatever command is running in a neutral grey.
    AppBadge {
        label: base.to_string(),
        color: OTHER_GREY,
        tier: AppTier::Other,
    }
}

fn tool(label: &str, color: Color) -> AppBadge {
    AppBadge {
        label: label.into(),
        color,
        tier: AppTier::Tool,
    }
}

/// Infer live activity from a pane's title.
/// Braille spinner prefix → Busy; `✳` prefix → Idle; anything else → Plain.
pub fn activity(title: &str) -> Activity {
    let trimmed = title.trim_start();
    let Some(first) = trimmed.chars().next() else {
        return Activity::Plain;
    };
    if is_braille(first) {
        Activity::Busy
    } else if first == '✳' {
        Activity::Idle
    } else {
        Activity::Plain
    }
}

fn is_braille(c: char) -> bool {
    matches!(c as u32, 0x2800..=0x28FF)
}

/// Strip a leading status symbol (`✳` or braille spinner) from a pane title
/// for clean display in the detail card.
pub fn clean_title(title: &str) -> &str {
    let trimmed = title.trim();
    match trimmed.chars().next() {
        Some(c) if is_braille(c) || c == '✳' => trimmed[c.len_utf8()..].trim_start(),
        _ => trimmed,
    }
}

/// Display a session name without this machine's `<host>_` prefix, when
/// removing it still leaves a non-empty name.
pub fn strip_host_prefix<'a>(name: &'a str, host_prefix: Option<&str>) -> &'a str {
    let Some(prefix) = host_prefix.filter(|p| !p.is_empty()) else {
        return name;
    };
    name.strip_prefix(prefix)
        .filter(|rest| !rest.is_empty())
        .unwrap_or(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_top_ai_apps_with_distinct_preset_colors() {
        let claude = detect("2.1.330", "✳ api-work");
        let agy = detect("antigravity", "agy --dangerously-skip-permissions");
        let codex = detect("codex", "");
        let gemini = detect("gemini", "");
        let opencode = detect("opencode", "");
        let aider = detect("aider", "");

        assert_eq!(
            (claude.label.as_str(), claude.tier),
            ("claude", AppTier::Ai)
        );
        assert_eq!((agy.label.as_str(), agy.tier), ("agy", AppTier::Ai));
        assert_eq!((codex.label.as_str(), codex.tier), ("codex", AppTier::Ai));
        assert_eq!(
            (gemini.label.as_str(), gemini.tier),
            ("gemini", AppTier::Ai)
        );
        assert_eq!(
            (opencode.label.as_str(), opencode.tier),
            ("opencode", AppTier::Ai)
        );
        assert_eq!((aider.label.as_str(), aider.tier), ("aider", AppTier::Ai));

        // Top AI apps all have distinct preset colours.
        let colors = [
            claude.color,
            agy.color,
            codex.color,
            gemini.color,
            opencode.color,
            aider.color,
        ];
        for i in 0..colors.len() {
            for j in (i + 1)..colors.len() {
                assert_ne!(colors[i], colors[j], "duplicate colour at {i} and {j}");
            }
        }
    }

    #[test]
    fn detects_claude_running_under_node_or_semver() {
        assert_eq!(detect("claude", "").label, "claude");
        assert_eq!(detect("2.1.328", "⠙ build").label, "claude");
        assert_eq!(detect("node", "✳ api-work").label, "claude");
        assert_eq!(detect("node", "server.js").label, "node");
        assert_eq!(detect("node", "server.js").tier, AppTier::Tool);
    }

    #[test]
    fn detects_top_popular_terminal_apps_and_shells() {
        let nvim = detect("nvim", "main.rs");
        let ssh = detect("ssh", "prod");
        let docker = detect("docker", "");
        let lazygit = detect("lazygit", "");
        let py = detect("python3.12", "");
        let cargo = detect("cargo", "");
        let go = detect("go", "");
        let psql = detect("psql", "");
        let btop = detect("btop", "");
        let zsh = detect("-zsh", "user@host");

        assert_eq!((nvim.label.as_str(), nvim.tier), ("nvim", AppTier::Tool));
        assert_eq!((ssh.label.as_str(), ssh.tier), ("ssh", AppTier::Tool));
        assert_eq!(
            (docker.label.as_str(), docker.tier),
            ("docker", AppTier::Tool)
        );
        assert_eq!(
            (lazygit.label.as_str(), lazygit.tier),
            ("lazygit", AppTier::Tool)
        );
        assert_eq!((py.label.as_str(), py.tier), ("python", AppTier::Tool));
        assert_eq!((cargo.label.as_str(), cargo.tier), ("cargo", AppTier::Tool));
        assert_eq!((go.label.as_str(), go.tier), ("go", AppTier::Tool));
        assert_eq!((psql.label.as_str(), psql.tier), ("psql", AppTier::Tool));
        assert_eq!((btop.label.as_str(), btop.tier), ("btop", AppTier::Tool));
        assert_eq!((zsh.label.as_str(), zsh.tier), ("zsh", AppTier::Shell));

        assert_ne!(nvim.color, ssh.color);
        assert_ne!(ssh.color, docker.color);
        assert_ne!(docker.color, lazygit.color);
        assert_ne!(py.color, cargo.color);
        assert_ne!(zsh.color, nvim.color);
    }

    #[test]
    fn infers_activity_and_cleans_titles() {
        assert_eq!(activity("⠙ em-gemini-model"), Activity::Busy);
        assert_eq!(activity("⠋ thinking"), Activity::Busy);
        assert_eq!(activity("✳ koo-studio"), Activity::Idle);
        assert_eq!(
            activity("agy --dangerously-skip-permissions"),
            Activity::Plain
        );
        assert_eq!(activity(""), Activity::Plain);

        assert_eq!(clean_title("⠙ em-gemini-model"), "em-gemini-model");
        assert_eq!(clean_title("✳ koo-studio"), "koo-studio");
        assert_eq!(clean_title("plain title"), "plain title");
    }

    #[test]
    fn strips_local_host_prefix_without_emptying_the_name() {
        assert_eq!(strip_host_prefix("box_api", Some("box_")), "api");
        assert_eq!(strip_host_prefix("other_api", Some("box_")), "other_api");
        assert_eq!(strip_host_prefix("box_", Some("box_")), "box_");
        assert_eq!(strip_host_prefix("api", Some("box_")), "api");
        assert_eq!(strip_host_prefix("box_api", None), "box_api");
    }
}
