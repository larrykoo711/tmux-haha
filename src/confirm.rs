//! Asking before doing something that can't be undone. Off a terminal there
//! is nobody to ask, so the answer is no unless `--yes` said yes.
//! Adapted from cc-moss (MIT, https://github.com/larrykoo711/cc-moss), `cmd/confirm.rs`.

use std::io::{BufRead, IsTerminal, Write};

use anyhow::{bail, Result};

/// Ask `question`; `refused` says how to go ahead without a terminal.
pub fn ask(yes: bool, question: &str, refused: &str, default_yes: bool) -> Result<bool> {
    if yes {
        return Ok(true);
    }
    if !std::io::stdin().is_terminal() {
        bail!(tr!(
            format!("not on a terminal, so ta can't ask; {refused}"),
            format!("当前不是交互式终端，没法问你；{refused}")
        ));
    }
    let hint = if default_yes { "[Y/n]" } else { "[y/N]" };
    eprint!("{question} {hint} ");
    std::io::stderr().flush()?;
    let mut answer = String::new();
    let read = std::io::stdin().lock().read_line(&mut answer)?;
    Ok(agrees(read, &answer, default_yes))
}

/// End of input (Ctrl-D, a closed pipe) is never a yes, whatever the default.
fn agrees(bytes_read: usize, answer: &str, default_yes: bool) -> bool {
    let answer = answer.trim().to_lowercase();
    bytes_read > 0
        && (matches!(answer.as_str(), "y" | "yes" | "是") || (default_yes && answer.is_empty()))
}

#[cfg(test)]
mod tests {
    use super::agrees;

    #[test]
    fn only_a_real_answer_is_yes() {
        assert!(agrees(2, "y\n", false));
        assert!(agrees(4, "YES\n", false));
        assert!(agrees(4, "是\n", false));
        assert!(!agrees(1, "\n", false));
        assert!(agrees(1, "\n", true));
        assert!(!agrees(0, "", true), "end of input is no");
        assert!(!agrees(2, "n\n", true));
    }
}
