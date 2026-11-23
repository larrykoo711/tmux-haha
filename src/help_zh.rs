//! Chinese `--help` and usage errors. Built only when something is shown.
//! Adapted from cc-moss (MIT, https://github.com/larrykoo711/cc-moss), `cc-moss/src/help_zh.rs`.

use clap::error::ErrorKind;
use clap::{Arg, ArgAction, Command};

/// The help layout with full-width headings; sections a command lacks are left out.
fn template(cmd: &Command) -> String {
    let mut t = String::from("{about-with-newline}\n用法：{usage}\n");
    if cmd.has_subcommands() {
        t.push_str("\n命令：\n{subcommands}\n");
    }
    if cmd.get_positionals().any(|a| !a.is_hide_set()) {
        t.push_str("\n参数：\n{positionals}\n");
    }
    t.push_str("\n选项：\n{options}{after-help}");
    t
}

const AFTER_HELP: &str = "不带命令直接运行 `ta` 会打开会话选择器。";

/// (command, argument id, help). An empty argument id is the command's own
/// description; an empty command is `ta` itself.
pub const TEXT: &[(&str, &str, &str)] = &[
    ("", "", "找到、新建、切换 tmux 会话，一个命令就够"),
    ("", "version", "显示版本"),
    (
        "",
        "socket",
        "用哪个 tmux server：socket 名字（tmux -L）或路径（tmux -S）；默认用 tmux 自己的",
    ),
    ("", "detached", "只新建或找到会话并打印名字，不进去"),
    (
        "",
        "name",
        "进入这个名字的会话，没有就在当前目录新建（会话名和命令同名时：ta -- ls）",
    ),
    ("new", "", "在当前目录新建会话，不给名字就用目录名"),
    ("new", "name", "会话名（默认用目录名）"),
    ("ls", "", "列出会话；* 表示正有终端接着"),
    ("version", "", "显示版本"),
];

/// `cmd` with Chinese text, headings and help flags, for help output.
pub fn localize(cmd: Command) -> Command {
    let mut cmd = translate(cmd, "").after_help(AFTER_HELP);
    let names: Vec<String> = cmd
        .get_subcommands()
        .map(|c| c.get_name().to_string())
        .collect();
    for name in names {
        cmd = cmd.mut_subcommand(&name, |c| translate(c, &name));
    }
    // clap adds its `help` command while building; translate it afterwards.
    cmd.build();
    cmd.mut_subcommand("help", |c| c.about("显示这份帮助，或某个命令的帮助"))
}

fn translate(cmd: Command, name: &str) -> Command {
    let template = template(&cmd);
    let mut cmd = cmd.help_template(template).disable_help_flag(true).arg(
        Arg::new("help")
            .short('h')
            .long("help")
            .action(ArgAction::Help)
            .help("显示帮助"),
    );
    for (_, arg, help) in TEXT.iter().filter(|(c, _, _)| *c == name) {
        cmd = if arg.is_empty() {
            cmd.about(*help).long_about(None::<&str>)
        } else {
            cmd.mut_arg(*arg, |a| a.help(*help))
        };
    }
    cmd
}

/// A usage mistake in plain Chinese.
pub fn mistake(error: &clap::Error) -> String {
    let what = crate::cli::offending(error).unwrap_or_default();
    match error.kind() {
        ErrorKind::UnknownArgument => format!("不认识的参数 `{what}`"),
        ErrorKind::InvalidSubcommand => format!("没有 `{what}` 这个命令"),
        ErrorKind::InvalidValue => format!("取值不对：{what}"),
        ErrorKind::MissingRequiredArgument => format!("少了必需的参数 {what}"),
        ErrorKind::ArgumentConflict => format!("这些参数不能一起用：{what}"),
        ErrorKind::TooManyValues | ErrorKind::WrongNumberOfValues => {
            format!("参数个数不对：{what}")
        }
        ErrorKind::NoEquals | ErrorKind::ValueValidation => format!("取值不对：{what}"),
        _ => {
            let text = error.to_string();
            text.lines()
                .next()
                .unwrap_or("")
                .trim_start_matches("error: ")
                .to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    #[test]
    fn every_translated_argument_exists() {
        // mut_arg panics on an unknown id; debug_assert checks the rest.
        let cmd = super::localize(crate::cli::Cli::command());
        cmd.clone().debug_assert();
        for (command, arg, _) in super::TEXT {
            let c = if command.is_empty() {
                &cmd
            } else {
                cmd.find_subcommand(command)
                    .unwrap_or_else(|| panic!("no command {command}"))
            };
            assert!(
                arg.is_empty() || c.get_arguments().any(|a| a.get_id() == *arg),
                "{command} has no argument {arg}"
            );
        }
    }

    #[test]
    fn every_command_has_chinese_text() {
        let cmd = crate::cli::Cli::command();
        for sub in cmd.get_subcommands() {
            let name = sub.get_name();
            assert!(
                super::TEXT
                    .iter()
                    .any(|(c, a, _)| *c == name && a.is_empty()),
                "{name} has no Chinese description"
            );
            for arg in sub.get_arguments() {
                let id = arg.get_id().as_str();
                if id == "help" || arg.is_global_set() {
                    continue;
                }
                assert!(
                    super::TEXT.iter().any(|(c, a, _)| *c == name && *a == id),
                    "{name} {id} has no Chinese help"
                );
            }
        }
    }
}
