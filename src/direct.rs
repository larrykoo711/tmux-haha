//! `ta <name>`: which session a name means.

use crate::naming;
use crate::tmux::SessionInfo;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A session with that name exists: its id (`$N`).
    Existing(String),
    /// None does: start one with this name.
    Create(String),
}

/// The session named exactly `wanted`, else the one named as tmux would
/// take `wanted` ("my notes" → `my-notes`), else a new one with that name.
/// Never a prefix match: `foo` does not mean `foo-bar`.
pub fn resolve(sessions: &[SessionInfo], wanted: &str) -> Target {
    let clean = naming::sanitize(wanted);
    [wanted, clean.as_str()]
        .iter()
        .find_map(|name| sessions.iter().find(|s| s.name == *name))
        .map_or(Target::Create(clean.clone()), |s| {
            Target::Existing(s.id.clone())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(id: &str, name: &str) -> SessionInfo {
        SessionInfo {
            id: id.into(),
            name: name.into(),
            windows: 1,
            activity_ms: 0,
            attached: false,
            path: "/".into(),
        }
    }

    #[test]
    fn direct_prefers_exact_existing_session() {
        let sessions = [session("$1", "foo-bar"), session("$2", "foo")];
        assert_eq!(resolve(&sessions, "foo"), Target::Existing("$2".into()));
    }

    #[test]
    fn sanitized_name_matches_existing() {
        let sessions = [session("$4", "my-notes")];
        assert_eq!(
            resolve(&sessions, "my notes"),
            Target::Existing("$4".into())
        );
    }

    #[test]
    fn absent_means_create() {
        let sessions = [session("$1", "foo-bar")];
        assert_eq!(resolve(&sessions, "foo"), Target::Create("foo".into()));
        assert_eq!(resolve(&[], "a b"), Target::Create("a-b".into()));
    }
}
