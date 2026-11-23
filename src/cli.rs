//! The command line: what `ta` takes, and help in the reader's language.

use std::ffi::OsString;

use clap::error::{ContextKind, ContextValue, ErrorKind};
use clap::{CommandFactory, Parser, Subcommand};

/// Find, start and switch tmux sessions, all from one command.
#[derive(Parser)]
#[command(
    name = "ta",
    about = "Find, start and switch tmux sessions, all from one command",
    after_help = "Run `ta` with no command to open the session picker.",
    disable_version_flag = true
)]
pub struct Cli {
    /// Print the version.
    #[arg(short = 'V', long)]
    pub version: bool,

    /// The tmux server: a socket name (`tmux -L`) or path (`tmux -S`). Default: tmux's own.
    #[arg(long, env = "TA_TMUX_SOCKET", global = true, value_name = "NAME|PATH")]
    pub socket: Option<String>,

    /// Start or find the session and print its name, without going there.
    #[arg(short, long, global = true)]
    pub detached: bool,

    #[command(subcommand)]
    pub command: Option<Cmd>,

    /// Go to the session with this name, starting it in this folder if there is none
    /// (`ta -- ls` for a session named like a command).
    pub name: Option<String>,
}

#[derive(Subcommand)]
pub enum Cmd {
    /// Start a new session in this folder, named after it unless you give a name.
    New {
        /// The session's name (default: the folder's).
        name: Option<String>,
        /// Start it in this folder instead of the current one.
        #[arg(long, value_name = "DIR")]
        dir: Option<std::path::PathBuf>,
    },
    /// List sessions; * marks one a terminal is showing.
    Ls {
        /// Machine-readable output (schema 1).
        #[arg(long)]
        json: bool,
    },
    /// Close a session and everything running in it (asks first).
    Close {
        /// The session to close.
        name: String,
        /// Don't ask.
        #[arg(short, long)]
        yes: bool,
    },
    /// Rename a session.
    Rename {
        /// The session to rename.
        name: String,
        /// Its new name (made one tmux can take; -2, -3… if taken).
        new_name: String,
    },
    /// Print the version.
    Version,
    /// Check that ta can do its job here: tmux, the server, the config, zoxide.
    Doctor,
    /// Show the config file and where each setting's value comes from.
    Config {
        /// Print only the config file's path.
        #[arg(long)]
        path: bool,
    },
}

/// The parsed command line. Help, version flags and usage mistakes are
/// shown here, in the reader's language, and end the process.
pub fn parse() -> Cli {
    let argv: Vec<OsString> = std::env::args_os().collect();
    match Cli::try_parse_from(&argv) {
        Ok(cli) => cli,
        Err(e) => explain(e, &argv),
    }
}

fn explain(error: clap::Error, argv: &[OsString]) -> ! {
    if crate::i18n::lang() != crate::i18n::Lang::Zh {
        error.exit()
    }
    match error.kind() {
        ErrorKind::DisplayHelp | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => {
            match crate::help_zh::localize(Cli::command()).try_get_matches_from(argv) {
                Err(zh) => zh.exit(),
                Ok(_) => error.exit(),
            }
        }
        ErrorKind::DisplayVersion => error.exit(),
        _ => {
            eprintln!("ta：{}\n", crate::help_zh::mistake(&error));
            let mut cmd = crate::help_zh::localize(Cli::command());
            let usage = cmd.render_usage().to_string();
            let usage = usage
                .trim_start_matches("Usage: ")
                .trim_start_matches("用法：");
            eprintln!("用法：{usage}\n\n更多信息：ta --help");
            std::process::exit(2)
        }
    }
}

/// The offending word clap found, if it says.
pub fn offending(error: &clap::Error) -> Option<String> {
    for kind in [
        ContextKind::InvalidArg,
        ContextKind::InvalidSubcommand,
        ContextKind::InvalidValue,
    ] {
        match error.get(kind) {
            Some(ContextValue::String(s)) => return Some(s.clone()),
            Some(ContextValue::Strings(v)) if !v.is_empty() => return Some(v.join(" ")),
            _ => {}
        }
    }
    None
}
