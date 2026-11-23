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
/// take `wanted` ("my notes" → `my-notes`), else that name with this
/// machine's prefix (`box_my-notes`) when there is one, else a new session
/// named as ta names new ones. Never a prefix match: `foo` is not `foo-bar`.
pub fn resolve(sessions: &[SessionInfo], wanted: &str, host: Option<&str>) -> Target {
    let clean = naming::sanitize(wanted);
    let prefixed = naming::session_name(host, wanted);
    [wanted, clean.as_str(), prefixed.as_str()]
        .iter()
        .find_map(|name| sessions.iter().find(|s| s.name == *name))
        .map_or(Target::Create(prefixed.clone()), |s| {
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
        assert_eq!(
            resolve(&sessions, "foo", None),
            Target::Existing("$2".into())
        );
    }

    #[test]
    fn sanitized_name_matches_existing() {
        let sessions = [session("$4", "my-notes")];
        assert_eq!(
            resolve(&sessions, "my notes", None),
            Target::Existing("$4".into())
        );
    }

    #[test]
    fn absent_means_create() {
        let sessions = [session("$1", "foo-bar")];
        assert_eq!(
            resolve(&sessions, "foo", None),
            Target::Create("foo".into())
        );
        assert_eq!(resolve(&[], "a b", None), Target::Create("a-b".into()));
    }

    #[test]
    fn prefixed_name_is_found() {
        let sessions = [session("$5", "box_work")];
        assert_eq!(
            resolve(&sessions, "work", Some("box")),
            Target::Existing("$5".into())
        );
        assert_eq!(
            resolve(&sessions, "api", Some("box")),
            Target::Create("box_api".into())
        );
        // Without a prefix configured, `work` is a new session.
        assert_eq!(
            resolve(&sessions, "work", None),
            Target::Create("work".into())
        );
    }
}
