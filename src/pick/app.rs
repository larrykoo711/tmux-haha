//! Picker state and key handling, independent of the terminal. Adapted from
//! cc-moss (MIT, https://github.com/larrykoo711/cc-moss), `cc-moss/src/pick/app.rs`.
//!
//! Made for people, not for muscle memory: type to search, ↑↓ to move,
//! Enter to open, → for everything else (a small menu), Esc to go back.
//! Sessions are grouped by folder: the folder is written once, and each
//! session takes one line.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

use crate::tmux::SessionInfo;

/// A list entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    /// Start a session here (named by the query, if any).
    New,
    Session(SessionInfo),
}

/// Sessions sharing a folder.
#[derive(Debug, Clone)]
pub struct Group {
    pub path: String,
    pub sessions: Vec<SessionInfo>,
}

/// One line of the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    New,
    Header(usize),
    /// Group, session within it.
    Session(usize, usize),
}

impl Row {
    fn selectable(self) -> bool {
        !matches!(self, Row::Header(_))
    }
}

/// What the → menu offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    Open,
    Rename,
    Close,
}

impl Choice {
    pub fn label(self) -> &'static str {
        match self {
            Choice::Open => tr!("Open", "进入"),
            Choice::Rename => tr!("Rename", "改名"),
            Choice::Close => tr!("Close", "关闭"),
        }
    }
}

/// What is on top of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Browse,
    Menu {
        choices: Vec<Choice>,
        cursor: usize,
    },
    /// "Close <name>?" with Close / Keep; `yes` is the highlighted button.
    Confirm {
        yes: bool,
    },
    Rename {
        text: String,
    },
}

/// What the caller should do after a key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    None,
    Quit,
    /// Go to session `$N`.
    Attach(String),
    /// Start a session in the current folder, named by the query if any.
    New {
        name: Option<String>,
    },
    /// Rename session `$N` (the name is cleaned by the caller).
    Rename {
        id: String,
        name: String,
    },
    /// End session `$N`.
    Close(String),
}

pub struct App {
    pub query: String,
    pub mode: Mode,
    groups: Vec<Group>,
    rows: Vec<Row>,
    /// Index into `rows`; always a selectable row.
    pub cursor: usize,
    pub message: Option<String>,
    /// Matching sessions / all sessions, for the search line.
    pub matched: usize,
    pub total: usize,
    /// The session this picker runs in (`$N`), which it must not close.
    pub here: Option<String>,
    matcher: Matcher,
}

/// Why the current session has no "Close".
pub fn here_hint() -> String {
    tr!(
        "You are in this session, so it can't be closed from here.",
        "你就在这个会话里，不能在这里关掉它。"
    )
    .to_string()
}

impl App {
    pub fn new(sessions: Vec<SessionInfo>, here: Option<String>) -> Self {
        let mut app = App {
            query: String::new(),
            mode: Mode::Browse,
            groups: Vec::new(),
            rows: Vec::new(),
            cursor: 0,
            message: None,
            matched: 0,
            total: 0,
            here,
            matcher: Matcher::new(Config::DEFAULT),
        };
        app.set_sessions(sessions);
        app
    }

    /// Replace the data, keeping the selection on the same session if it still exists.
    pub fn set_sessions(&mut self, sessions: Vec<SessionInfo>) {
        let keep = self.selected_id();
        self.group(sessions);
        if let Some(keep) = keep {
            if let Some(i) = (0..self.rows.len()).find(|&i| self.id_at(i) == Some(&keep)) {
                self.cursor = i;
            }
        }
    }

    fn group(&mut self, sessions: Vec<SessionInfo>) {
        let mut groups: Vec<Group> = Vec::new();
        for s in sessions {
            match groups.iter_mut().find(|g| g.path == s.path) {
                Some(g) => g.sessions.push(s),
                None => groups.push(Group {
                    path: s.path.clone(),
                    sessions: vec![s],
                }),
            }
        }
        // Most recently active first, for groups and for sessions in them.
        for g in &mut groups {
            g.sessions.sort_by_key(|s| std::cmp::Reverse(s.activity_ms));
        }
        groups.sort_by_key(|g| {
            std::cmp::Reverse(g.sessions.first().map_or(i64::MIN, |s| s.activity_ms))
        });
        self.groups = groups;
        self.refilter();
    }

    pub fn groups(&self) -> &[Group] {
        &self.groups
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    pub fn session(&self, row: Row) -> Option<&SessionInfo> {
        match row {
            Row::Session(g, i) => self.groups.get(g)?.sessions.get(i),
            _ => None,
        }
    }

    pub fn selected(&self) -> Option<Item> {
        match *self.rows.get(self.cursor)? {
            Row::New => Some(Item::New),
            row => self.session(row).cloned().map(Item::Session),
        }
    }

    fn selected_id(&self) -> Option<String> {
        self.id_at(self.cursor).cloned()
    }

    fn id_at(&self, row: usize) -> Option<&String> {
        self.session(*self.rows.get(row)?).map(|s| &s.id)
    }

    pub fn is_here(&self, s: &SessionInfo) -> bool {
        self.here.as_deref() == Some(s.id.as_str())
    }

    /// Search runs over names and folders; groups keep their order unless
    /// searching, sessions in a group are ranked by score.
    fn refilter(&mut self) {
        let query = self.query.trim().to_string();
        let mut rows = Vec::new();
        // "New" comes first, one step up from the first session. An exact
        // name means the session exists: names are unique in tmux.
        if query.is_empty() || !self.names_exactly(&query) {
            rows.push(Row::New);
        }
        let pattern = Pattern::parse(&query, CaseMatching::Ignore, Normalization::Smart);
        let mut buf = Vec::new();
        let (mut matched, mut total) = (0, 0);
        let mut hits_by_group: Vec<(usize, Vec<(usize, u32)>)> = Vec::new();
        for (g, group) in self.groups.iter().enumerate() {
            total += group.sessions.len();
            let mut hits: Vec<(usize, u32)> = group
                .sessions
                .iter()
                .enumerate()
                .filter_map(|(i, s)| {
                    if query.is_empty() {
                        return Some((i, 0));
                    }
                    // The folder's own name, not its whole path: a fuzzy match
                    // finds most short words somewhere in a long path.
                    let folder = s.path.rsplit('/').find(|p| !p.is_empty()).unwrap_or("");
                    let text = format!("{} {folder}", s.name);
                    pattern
                        .score(Utf32Str::new(&text, &mut buf), &mut self.matcher)
                        .map(|score| (i, score))
                })
                .collect();
            if hits.is_empty() {
                continue;
            }
            if !query.is_empty() {
                hits.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            }
            matched += hits.len();
            hits_by_group.push((g, hits));
        }
        if !query.is_empty() {
            hits_by_group.sort_by(|a, b| b.1[0].1.cmp(&a.1[0].1).then(a.0.cmp(&b.0)));
        }
        for (g, hits) in hits_by_group {
            rows.push(Row::Header(g));
            rows.extend(hits.into_iter().map(|(i, _)| Row::Session(g, i)));
        }
        self.rows = rows;
        self.matched = matched;
        self.total = total;
        // Land on the first (best) session, else on "new".
        self.cursor = self
            .rows
            .iter()
            .position(|r| matches!(r, Row::Session(..)))
            .or_else(|| self.rows.iter().position(|r| *r == Row::New))
            .unwrap_or(0);
    }

    fn names_exactly(&self, query: &str) -> bool {
        self.groups
            .iter()
            .flat_map(|g| &g.sessions)
            .any(|s| s.name == query)
    }

    /// Move to the next selectable row, without wrapping (like a list should).
    fn move_cursor(&mut self, forward: bool) {
        let mut i = self.cursor;
        loop {
            let next = if forward {
                i + 1
            } else {
                match i.checked_sub(1) {
                    Some(n) => n,
                    None => return,
                }
            };
            match self.rows.get(next) {
                Some(r) if r.selectable() => {
                    self.cursor = next;
                    return;
                }
                Some(_) => i = next,
                None => return,
            }
        }
    }

    /// Select a row by index (mouse); true if it was already selected.
    pub fn click(&mut self, row: usize) -> bool {
        if self.mode != Mode::Browse || !self.rows.get(row).is_some_and(|r| r.selectable()) {
            return false;
        }
        let again = self.cursor == row;
        self.cursor = row;
        again
    }

    pub fn scroll(&mut self, down: bool) {
        if self.mode == Mode::Browse {
            self.move_cursor(down);
        }
    }

    pub fn handle(&mut self, key: KeyEvent) -> Action {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && key.code == KeyCode::Char('c') {
            return Action::Quit;
        }
        self.message = None;
        match self.mode.clone() {
            Mode::Browse => self.browse(key, ctrl),
            Mode::Menu { choices, cursor } => self.menu(key, choices, cursor),
            Mode::Confirm { yes } => self.confirm(key, yes),
            Mode::Rename { text } => self.rename(key, text),
        }
    }

    /// Enter (or a second click): open what is selected.
    pub fn open(&mut self) -> Action {
        match self.selected() {
            None => Action::None,
            Some(Item::New) => Action::New {
                name: Some(self.query.trim().to_string()).filter(|q| !q.is_empty()),
            },
            Some(Item::Session(s)) => Action::Attach(s.id),
        }
    }

    fn choices(&self, s: &SessionInfo) -> Vec<Choice> {
        if self.is_here(s) {
            vec![Choice::Open, Choice::Rename]
        } else {
            vec![Choice::Open, Choice::Rename, Choice::Close]
        }
    }

    fn browse(&mut self, key: KeyEvent, ctrl: bool) -> Action {
        match key.code {
            KeyCode::Esc if !self.query.is_empty() => {
                self.query.clear();
                self.refilter();
                Action::None
            }
            KeyCode::Esc => Action::Quit,
            KeyCode::Up => {
                self.move_cursor(false);
                Action::None
            }
            KeyCode::Down => {
                self.move_cursor(true);
                Action::None
            }
            // Not advertised; there for people who expect them.
            KeyCode::Char('p') if ctrl => {
                self.move_cursor(false);
                Action::None
            }
            KeyCode::Char('n') if ctrl => {
                self.move_cursor(true);
                Action::None
            }
            KeyCode::Right => {
                if let Some(Item::Session(s)) = self.selected() {
                    if self.is_here(&s) {
                        self.message = Some(here_hint());
                    }
                    self.mode = Mode::Menu {
                        choices: self.choices(&s),
                        cursor: 0,
                    };
                }
                Action::None
            }
            KeyCode::Enter => self.open(),
            // Every printable character is search input.
            KeyCode::Char(c) if !ctrl && !key.modifiers.contains(KeyModifiers::ALT) => {
                self.query.push(c);
                self.refilter();
                Action::None
            }
            KeyCode::Backspace => {
                self.query.pop();
                self.refilter();
                Action::None
            }
            _ => Action::None,
        }
    }

    fn menu(&mut self, key: KeyEvent, choices: Vec<Choice>, cursor: usize) -> Action {
        let last = choices.len().saturating_sub(1);
        match key.code {
            KeyCode::Esc | KeyCode::Left => {
                self.mode = Mode::Browse;
                Action::None
            }
            KeyCode::Up | KeyCode::Down => {
                let cursor = if key.code == KeyCode::Up {
                    cursor.saturating_sub(1)
                } else {
                    (cursor + 1).min(last)
                };
                self.mode = Mode::Menu { choices, cursor };
                Action::None
            }
            KeyCode::Enter => {
                self.mode = Mode::Browse;
                match (choices.get(cursor).copied(), self.selected()) {
                    (Some(Choice::Open), _) => self.open(),
                    (Some(Choice::Rename), Some(Item::Session(s))) => {
                        self.mode = Mode::Rename { text: s.name };
                        Action::None
                    }
                    (Some(Choice::Close), Some(Item::Session(_))) => {
                        // Keep is the default: closing a session cannot be undone.
                        self.mode = Mode::Confirm { yes: false };
                        Action::None
                    }
                    _ => Action::None,
                }
            }
            _ => Action::None,
        }
    }

    fn confirm(&mut self, key: KeyEvent, yes: bool) -> Action {
        let close = |app: &mut App| {
            app.mode = Mode::Browse;
            match app.selected() {
                Some(Item::Session(s)) if app.is_here(&s) => {
                    app.message = Some(here_hint());
                    Action::None
                }
                Some(Item::Session(s)) => Action::Close(s.id),
                _ => Action::None,
            }
        };
        match key.code {
            KeyCode::Left | KeyCode::Right | KeyCode::Tab => {
                self.mode = Mode::Confirm { yes: !yes };
                Action::None
            }
            KeyCode::Char('y') | KeyCode::Char('Y') => close(self),
            KeyCode::Enter if yes => close(self),
            KeyCode::Enter | KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                self.mode = Mode::Browse;
                Action::None
            }
            _ => Action::None,
        }
    }

    fn rename(&mut self, key: KeyEvent, mut text: String) -> Action {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Browse;
                Action::None
            }
            KeyCode::Enter => {
                self.mode = Mode::Browse;
                let name = text.trim().to_string();
                match self.selected() {
                    Some(Item::Session(s)) if !name.is_empty() && name != s.name => {
                        Action::Rename { id: s.id, name }
                    }
                    _ => Action::None,
                }
            }
            KeyCode::Backspace => {
                text.pop();
                self.mode = Mode::Rename { text };
                Action::None
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                text.push(c);
                self.mode = Mode::Rename { text };
                Action::None
            }
            _ => Action::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn typed(app: &mut App, text: &str) {
        for c in text.chars() {
            app.handle(key(KeyCode::Char(c)));
        }
    }

    pub fn session(id: &str, name: &str, path: &str, at: i64) -> SessionInfo {
        SessionInfo {
            id: id.into(),
            name: name.into(),
            windows: 1,
            activity_ms: at,
            attached: false,
            path: path.into(),
        }
    }

    /// /w/acme: api (5), web (9); /w/billing: scratch (1)
    fn sample(here: Option<&str>) -> App {
        App::new(
            vec![
                session("$1", "api", "/w/acme", 5),
                session("$2", "web", "/w/acme", 9),
                session("$3", "scratch", "/w/billing", 1),
            ],
            here.map(String::from),
        )
    }

    fn select(app: &mut App, name: &str) {
        typed(app, name);
        assert_eq!(
            app.selected(),
            app.groups()
                .iter()
                .flat_map(|g| &g.sessions)
                .find(|s| s.name == name)
                .cloned()
                .map(Item::Session)
        );
    }

    #[test]
    fn groups_by_folder_most_recent_first() {
        let app = sample(None);
        let names: Vec<&str> = app
            .rows()
            .iter()
            .filter_map(|r| app.session(*r).map(|s| s.name.as_str()))
            .collect();
        assert_eq!(names, ["web", "api", "scratch"]);
        assert_eq!(app.rows()[0], Row::New);
        // The cursor starts on the most recent session.
        assert_eq!(
            app.selected(),
            Some(Item::Session(session("$2", "web", "/w/acme", 9)))
        );
    }

    #[test]
    fn keyboard_moves_skip_headers_and_stop_at_the_ends() {
        let mut app = sample(None);
        app.handle(key(KeyCode::Up));
        assert_eq!(app.selected(), Some(Item::New));
        app.handle(key(KeyCode::Up));
        assert_eq!(app.selected(), Some(Item::New));
        for _ in 0..10 {
            app.handle(key(KeyCode::Down));
        }
        assert!(matches!(app.selected(), Some(Item::Session(s)) if s.name == "scratch"));
    }

    #[test]
    fn keyboard_enter_opens_the_selected_session() {
        let mut app = sample(None);
        select(&mut app, "api");
        assert_eq!(app.handle(key(KeyCode::Enter)), Action::Attach("$1".into()));
    }

    #[test]
    fn keyboard_enter_on_no_match_starts_new() {
        let mut app = sample(None);
        typed(&mut app, "zzz notes");
        assert_eq!(app.selected(), Some(Item::New));
        assert_eq!(
            app.handle(key(KeyCode::Enter)),
            Action::New {
                name: Some("zzz notes".into())
            }
        );
        // An exact name means the session exists: no "new" line.
        let mut app = sample(None);
        typed(&mut app, "api");
        assert!(!app.rows().contains(&Row::New));
    }

    #[test]
    fn keyboard_esc_clears_then_quits() {
        let mut app = sample(None);
        typed(&mut app, "ap");
        assert_eq!(app.handle(key(KeyCode::Esc)), Action::None);
        assert!(app.query.is_empty());
        assert_eq!(app.handle(key(KeyCode::Esc)), Action::Quit);
    }

    #[test]
    fn keyboard_menu_closes_back_to_the_list() {
        let mut app = sample(None);
        app.handle(key(KeyCode::Right));
        assert!(matches!(app.mode, Mode::Menu { .. }));
        app.handle(key(KeyCode::Left));
        assert_eq!(app.mode, Mode::Browse);
        app.handle(key(KeyCode::Right));
        app.handle(key(KeyCode::Esc));
        assert_eq!(app.mode, Mode::Browse);
    }

    #[test]
    fn keyboard_close_needs_confirmation() {
        let mut app = sample(None);
        select(&mut app, "scratch");
        app.handle(key(KeyCode::Right));
        app.handle(key(KeyCode::Down));
        app.handle(key(KeyCode::Down));
        assert_eq!(app.handle(key(KeyCode::Enter)), Action::None);
        assert_eq!(app.mode, Mode::Confirm { yes: false });
        app.handle(key(KeyCode::Left));
        assert_eq!(app.handle(key(KeyCode::Enter)), Action::Close("$3".into()));
        assert_eq!(app.mode, Mode::Browse);
    }

    #[test]
    fn confirm_defaults_to_keep() {
        for cancel in [KeyCode::Enter, KeyCode::Esc, KeyCode::Char('n')] {
            let mut app = sample(None);
            select(&mut app, "scratch");
            app.handle(key(KeyCode::Right));
            app.handle(key(KeyCode::Down));
            app.handle(key(KeyCode::Down));
            app.handle(key(KeyCode::Enter));
            assert_eq!(app.handle(key(cancel)), Action::None, "{cancel:?}");
            assert_eq!(app.mode, Mode::Browse);
        }
    }

    #[test]
    fn current_session_cannot_be_closed() {
        let mut app = sample(Some("$1"));
        select(&mut app, "api");
        app.handle(key(KeyCode::Right));
        assert_eq!(
            app.mode,
            Mode::Menu {
                choices: vec![Choice::Open, Choice::Rename],
                cursor: 0
            }
        );
        assert_eq!(app.message, Some(here_hint()));
        // Even if a confirmation were somehow open, it does not close here.
        app.mode = Mode::Confirm { yes: true };
        assert_eq!(app.handle(key(KeyCode::Enter)), Action::None);
        assert_eq!(app.message, Some(here_hint()));
    }

    #[test]
    fn rename_prefills_and_cleans() {
        let mut app = sample(None);
        select(&mut app, "api");
        app.handle(key(KeyCode::Right));
        app.handle(key(KeyCode::Down));
        app.handle(key(KeyCode::Enter));
        assert_eq!(app.mode, Mode::Rename { text: "api".into() });
        typed(&mut app, " v2");
        assert_eq!(
            app.handle(key(KeyCode::Enter)),
            Action::Rename {
                id: "$1".into(),
                name: "api v2".into()
            }
        );
        // Unchanged or empty: nothing to do.
        let mut app = sample(None);
        select(&mut app, "api");
        app.mode = Mode::Rename { text: "api".into() };
        assert_eq!(app.handle(key(KeyCode::Enter)), Action::None);
        app.mode = Mode::Rename { text: "  ".into() };
        assert_eq!(app.handle(key(KeyCode::Enter)), Action::None);
    }

    #[test]
    fn refresh_keeps_the_selection() {
        let mut app = sample(None);
        select(&mut app, "scratch");
        app.query.clear();
        app.set_sessions(vec![
            session("$3", "scratch", "/w/billing", 1),
            session("$4", "new-one", "/w/x", 99),
        ]);
        assert!(matches!(app.selected(), Some(Item::Session(s)) if s.id == "$3"));
    }

    #[test]
    fn click_selects_then_opens() {
        let mut app = sample(None);
        let row = app
            .rows()
            .iter()
            .position(|r| app.session(*r).is_some_and(|s| s.name == "scratch"))
            .unwrap();
        assert!(!app.click(row));
        assert!(app.click(row));
        assert_eq!(app.open(), Action::Attach("$3".into()));
        // Headers are not selectable.
        let header = app
            .rows()
            .iter()
            .position(|r| matches!(r, Row::Header(_)))
            .unwrap();
        assert!(!app.click(header));
    }

    #[test]
    fn search_ignores_the_rest_of_the_path() {
        let mut app = App::new(
            vec![
                session("$1", "api-gateway", "/private/tmp/acme", 2),
                session("$2", "账单", "/private/tmp/billing", 1),
            ],
            None,
        );
        typed(&mut app, "api");
        assert_eq!(app.matched, 1);
        // The folder's own name still counts.
        let mut app = sample(None);
        typed(&mut app, "billing");
        assert!(matches!(app.selected(), Some(Item::Session(s)) if s.name == "scratch"));
    }

    #[test]
    fn empty_server_offers_new() {
        let mut app = App::new(vec![], None);
        assert_eq!(app.rows(), [Row::New]);
        assert_eq!(app.handle(key(KeyCode::Enter)), Action::New { name: None });
    }
}
