//! `ta version`: the name, version, one line and the address in a
//! terminal; piped or redirected, the plain `ta <version>` scripts expect.

use std::io::IsTerminal;

const ACCENT: &str = "\x1b[1;38;2;120;200;120m";
const DIM: &str = "\x1b[2m";
const RESET: &str = "\x1b[0m";

pub fn print() {
    let version = env!("CARGO_PKG_VERSION");
    if std::io::stdout().is_terminal() {
        print!(
            "{}",
            banner(version, std::env::var_os("NO_COLOR").is_none())
        );
    } else {
        println!("ta {version}");
    }
}

pub fn line() -> &'static str {
    tr!(
        "Find, start and switch tmux sessions, all from one command.",
        "找到、新建、切换 tmux 会话，一个命令就够。"
    )
}

fn banner(version: &str, color: bool) -> String {
    let paint = |code: &str, text: &str| {
        if color {
            format!("{code}{text}{RESET}")
        } else {
            text.to_string()
        }
    };
    format!(
        "\n  {}  {}\n  {}\n  {}\n\n",
        paint(ACCENT, "ta"),
        paint(DIM, version),
        line(),
        paint(DIM, "github.com/larrykoo711/tmux-haha")
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn banner_has_the_name_version_and_line() {
        crate::i18n::set_for_test(crate::i18n::Lang::En);
        let plain = super::banner("9.9.9", false);
        assert!(plain.contains("ta  9.9.9"), "{plain}");
        assert!(plain.contains("all from one command."), "{plain}");
        assert!(
            plain.contains("github.com/larrykoo711/tmux-haha"),
            "{plain}"
        );
        assert!(!plain.contains('\x1b'), "no colour when asked not to");
        crate::i18n::set_for_test(crate::i18n::Lang::Zh);
        let zh = super::banner("9.9.9", true);
        crate::i18n::set_for_test(crate::i18n::Lang::En);
        assert!(zh.contains("一个命令就够") && zh.contains('\x1b'), "{zh}");
    }
}
