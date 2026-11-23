//! Blocks of a config file that ta owns, between marker lines. The rest of
//! the file belongs to the user: a block is added or replaced as a whole,
//! and `parts` lets the caller take it out again.
//! Adapted from cc-moss (MIT, https://github.com/larrykoo711/cc-moss), `moss-core/src/managed_block.rs`.

use anyhow::{bail, Result};

fn markers(tag: &str) -> (String, String) {
    (format!("# >>> {tag} >>>"), format!("# <<< {tag} <<<"))
}

/// The block's text, with the file's own line endings.
fn block(tag: &str, body: &str, crlf: bool) -> String {
    let (open, close) = markers(tag);
    let body = if body.ends_with('\n') || body.is_empty() {
        body.to_string()
    } else {
        format!("{body}\n")
    };
    let text = format!("{open}\n{body}{close}\n");
    if crlf {
        text.replace("\r\n", "\n").replace('\n', "\r\n")
    } else {
        text
    }
}

/// The file's lines with their endings, and the same lines bare.
fn split(text: &str) -> (Vec<&str>, Vec<&str>) {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let bare = lines
        .iter()
        .map(|l| l.trim_end_matches('\n').trim_end_matches('\r'))
        .collect();
    (lines, bare)
}

fn is_crlf(text: &str) -> bool {
    text.contains("\r\n")
}

/// Whether the block is there (and well formed).
pub fn contains(text: &str, tag: &str) -> bool {
    let (_, bare) = split(text);
    matches!(find(&bare, tag), Ok(Some(_)))
}

/// Line range `[start, end]` (inclusive) of the block, if there.
fn find(lines: &[&str], tag: &str) -> Result<Option<(usize, usize)>> {
    let (open, close) = markers(tag);
    let opens: Vec<usize> = (0..lines.len()).filter(|&i| lines[i] == open).collect();
    let closes: Vec<usize> = (0..lines.len()).filter(|&i| lines[i] == close).collect();
    match (opens.as_slice(), closes.as_slice()) {
        ([], []) => Ok(None),
        ([o], [c]) if o < c => Ok(Some((*o, *c))),
        _ => bail!(
            "the `{tag}` block markers (`# >>> {tag} >>>` … `# <<< {tag} <<<`) are duplicated or unbalanced; fix them by hand"
        ),
    }
}

/// Add the block at the end of the file, or replace it where it is.
pub fn upsert(text: &str, tag: &str, body: &str) -> Result<String> {
    let block = block(tag, body, is_crlf(text));
    let (lines, bare) = split(text);
    Ok(match find(&bare, tag)? {
        Some((start, end)) => format!(
            "{}{block}{}",
            lines[..start].concat(),
            lines[end + 1..].concat()
        ),
        None => {
            // Keep the file's last line intact if it lacks a newline.
            let mut head = text.to_string();
            if !head.is_empty() && !head.ends_with('\n') {
                head.push_str(if is_crlf(text) { "\r\n" } else { "\n" });
            }
            format!("{head}{block}")
        }
    })
}

/// The file around the block and the block's body: `(before, body, after)`.
pub fn parts(text: &str, tag: &str) -> Result<Option<(String, String, String)>> {
    let (lines, bare) = split(text);
    Ok(find(&bare, tag)?.map(|(start, end)| {
        (
            lines[..start].concat(),
            lines[start + 1..end].concat(),
            lines[end + 1..].concat(),
        )
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_replace_remove() {
        let user = "export A=1\nalias x=y\n";
        let once = upsert(user, "ta: t", "line 1").unwrap();
        assert_eq!(
            once,
            "export A=1\nalias x=y\n# >>> ta: t >>>\nline 1\n# <<< ta: t <<<\n"
        );
        let again = upsert(&once, "ta: t", "line 2").unwrap();
        assert_eq!(again.matches(">>> ta: t >>>").count(), 1);
        assert!(again.contains("line 2") && !again.contains("line 1"));
        let (before, body, after) = parts(&again, "ta: t").unwrap().unwrap();
        assert_eq!(
            (before.as_str(), body.as_str(), after.as_str()),
            (user, "line 2\n", "")
        );
        assert!(contains(&again, "ta: t") && !contains(user, "ta: t"));
    }

    #[test]
    fn broken_markers_are_left_for_a_person() {
        let broken = "# >>> ta: t >>>\nx\n";
        assert!(upsert(broken, "ta: t", "y").is_err());
        assert!(parts(broken, "ta: t").is_err());
    }

    #[test]
    fn crlf_files_stay_crlf() {
        let out = upsert("a\r\n", "ta: t", "b").unwrap();
        assert_eq!(out, "a\r\n# >>> ta: t >>>\r\nb\r\n# <<< ta: t <<<\r\n");
    }
}
