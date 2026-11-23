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

use crate::tmux::{SessionInfo, WindowInfo};

/// A list entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    /// Start a session here (named by the query, if any).
    New,
    Session(SessionInfo),
    Window(WindowInfo),
    /// A folder, and the session already started there, if any.
    Folder {
        path: String,
        session: Option<SessionInfo>,
    },
}

/// Which list the picker shows. Tab moves between them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Sessions,
    Windows,
    Folders,
}

impl Tab {
    pub const ALL: [Tab; 3] = [Tab::Sessions, Tab::Windows, Tab::Folders];

    pub fn label(self) -> &'static str {
        match self {
            Tab::Sessions => tr!("Sessions", "会话"),
            Tab::Windows => tr!("Windows", "窗口"),
            Tab::Folders => tr!("Folders", "目录"),
        }
    }

    fn step(self, back: bool) -> Tab {
        let i = Tab::ALL.iter().position(|t| *t == self).unwrap_or(0);
        let n = Tab::ALL.len();
        Tab::ALL[if back { (i + n - 1) % n } else { (i + 1) % n }]
    }
}

/// A session's windows, for the windows tab.
#[derive(Debug, Clone)]
pub struct WindowGroup {
    pub session_name: String,
    pub windows: Vec<WindowInfo>,
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
    /// A session's name above its windows.
    WindowHeader(usize),
    /// Window group, window within it.
    Window(usize, usize),
    /// Index into the folders.
    Folder(usize),
}

impl Row {
    fn selectable(self) -> bool {
        !matches!(self, Row::Header(_) | Row::WindowHeader(_))
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
    /// Go to window `@N` of session `$N`.
    AttachWindow {
        session: String,
        window: String,
    },
    /// Start a session in this folder.
    NewIn(String),
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
    pub tab: Tab,
    groups: Vec<Group>,
    window_groups: Vec<WindowGroup>,
    folders: Vec<String>,
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

/// Why a session another tool manages has nothing but "Open".
pub fn managed_hint(tool: &str) -> String {
    tr!(
        format!("{tool} manages this session; in ta you can only go to it."),
        format!("这个会话由 {tool} 管理，在 ta 里只能进入。")
    )
}

impl App {
    pub fn new(sessions: Vec<SessionInfo>, here: Option<String>) -> Self {
        let mut app = App {
            query: String::new(),
            mode: Mode::Browse,
            tab: Tab::Sessions,
            groups: Vec::new(),
            window_groups: Vec::new(),
            folders: Vec::new(),
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
        self.keeping_selection(|app| app.group(sessions));
    }

    /// Replace the windows, keeping the selection on the same window if it still exists.
    pub fn set_windows(&mut self, windows: Vec<WindowInfo>) {
        self.keeping_selection(|app| {
            let mut groups: Vec<WindowGroup> = Vec::new();
            let mut ids: Vec<String> = Vec::new();
            for w in windows {
                match ids.iter().position(|id| *id == w.session_id) {
                    Some(g) => groups[g].windows.push(w),
                    None => {
                        ids.push(w.session_id.clone());
                        groups.push(WindowGroup {
                            session_name: w.session_name.clone(),
                            windows: vec![w],
                        });
                    }
                }
            }
            for g in &mut groups {
                g.windows.sort_by_key(|w| w.index);
            }
            let latest = |g: &WindowGroup| g.windows.iter().map(|w| w.activity_ms).max();
            groups.sort_by_key(|g| std::cmp::Reverse(latest(g)));
            app.window_groups = groups;
            app.refilter();
        });
    }

    /// The folder tab's folders, in the order they are offered.
    pub fn set_folders(&mut self, folders: Vec<String>) {
        self.keeping_selection(|app| {
            app.folders = folders;
            app.refilter();
        });
    }

    /// The session started in `path`, if any: the most recently active one.
    pub fn session_in(&self, path: &str) -> Option<&SessionInfo> {
        self.groups
            .iter()
            .find(|g| g.path == path)
            .and_then(|g| g.sessions.first())
    }

    pub fn folder(&self, row: Row) -> Option<&str> {
        match row {
            Row::Folder(i) => self.folders.get(i).map(String::as_str),
            _ => None,
        }
    }

    fn keeping_selection(&mut self, change: impl FnOnce(&mut Self)) {
        let keep = self.key_at(self.cursor);
        change(self);
        if let Some(keep) = keep {
            if let Some(i) = (0..self.rows.len()).find(|&i| self.key_at(i) == Some(keep.clone())) {
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

    pub fn window_groups(&self) -> &[WindowGroup] {
        &self.window_groups
    }

    pub fn window(&self, row: Row) -> Option<&WindowInfo> {
        match row {
            Row::Window(g, i) => self.window_groups.get(g)?.windows.get(i),
            _ => None,
        }
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
            row @ Row::Window(..) => self.window(row).cloned().map(Item::Window),
            row @ Row::Folder(_) => self.folder(row).map(|path| Item::Folder {
                path: path.to_string(),
                session: self.session_in(path).cloned(),
            }),
            row => self.session(row).cloned().map(Item::Session),
        }
    }

    /// What a row stands for, to find it again after a refresh: `$N` or `@N`.
    fn key_at(&self, row: usize) -> Option<String> {
        let row = *self.rows.get(row)?;
        self.session(row)
            .map(|s| s.id.clone())
            .or_else(|| self.window(row).map(|w| w.id.clone()))
            .or_else(|| self.folder(row).map(String::from))
    }

    pub fn is_here(&self, s: &SessionInfo) -> bool {
        self.here.as_deref() == Some(s.id.as_str())
    }

    /// Search runs over names and folders; groups keep their order unless
    /// searching, sessions in a group are ranked by score.
    fn refilter(&mut self) {
        match self.tab {
            Tab::Windows => return self.refilter_windows(),
            Tab::Folders => return self.refilter_folders(),
            Tab::Sessions => {}
        }
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

    /// Windows match on their own name and their session's.
    fn refilter_windows(&mut self) {
        let query = self.query.trim().to_string();
        let pattern = Pattern::parse(&query, CaseMatching::Ignore, Normalization::Smart);
        let mut buf = Vec::new();
        let (mut rows, mut matched, mut total) = (Vec::new(), 0, 0);
        for (g, group) in self.window_groups.iter().enumerate() {
            total += group.windows.len();
            let hits: Vec<usize> = group
                .windows
                .iter()
                .enumerate()
                .filter(|(_, w)| {
                    query.is_empty() || {
                        let text = format!("{} {}", w.name, group.session_name);
                        pattern
                            .score(Utf32Str::new(&text, &mut buf), &mut self.matcher)
                            .is_some()
                    }
                })
                .map(|(i, _)| i)
                .collect();
            if hits.is_empty() {
                continue;
            }
            matched += hits.len();
            rows.push(Row::WindowHeader(g));
            rows.extend(hits.into_iter().map(|i| Row::Window(g, i)));
        }
        self.rows = rows;
        self.matched = matched;
        self.total = total;
        self.cursor = self
            .rows
            .iter()
            .position(|r| matches!(r, Row::Window(..)))
            .unwrap_or(0);
    }

    /// Folders match on their own name and their parent's; their order stays.
    fn refilter_folders(&mut self) {
        let query = self.query.trim().to_string();
        let pattern = Pattern::parse(&query, CaseMatching::Ignore, Normalization::Smart);
        let mut buf = Vec::new();
        let mut hits: Vec<(usize, u32)> = self
            .folders
            .iter()
            .enumerate()
            .filter_map(|(i, path)| {
                if query.is_empty() {
                    return Some((i, 0));
                }
                let mut parts = path.rsplit('/').filter(|p| !p.is_empty());
                let name = parts.next().unwrap_or("");
                let text = format!("{name} {}", parts.next().unwrap_or(""));
                pattern
                    .score(Utf32Str::new(&text, &mut buf), &mut self.matcher)
                    .map(|score| (i, score))
            })
            .collect();
        if !query.is_empty() {
            hits.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        }
        self.matched = hits.len();
        self.total = self.folders.len();
        self.rows = hits.into_iter().map(|(i, _)| Row::Folder(i)).collect();
        self.cursor = 0;
    }

    /// Show `tab`, with an empty search.
    fn switch_to(&mut self, tab: Tab) {
        self.tab = tab;
        self.query.clear();
        self.refilter();
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
            Some(Item::Window(w)) => Action::AttachWindow {
                session: w.session_id,
                window: w.id,
            },
            Some(Item::Folder {
                session: Some(s), ..
            }) => Action::Attach(s.id),
            Some(Item::Folder {
                path,
                session: None,
            }) => Action::NewIn(path),
        }
    }

    fn choices(&self, s: &SessionInfo) -> Vec<Choice> {
        if s.managed_by.is_some() {
            vec![Choice::Open]
        } else if self.is_here(s) {
            vec![Choice::Open, Choice::Rename]
        } else {
            vec![Choice::Open, Choice::Rename, Choice::Close]
        }
    }

    /// Why `s` can't be changed from here, if it can't: another tool manages it.
    fn hands_off(s: &SessionInfo) -> Option<String> {
        s.managed_by.as_deref().map(managed_hint)
    }

    fn browse(&mut self, key: KeyEvent, ctrl: bool) -> Action {
        match key.code {
            KeyCode::Esc if !self.query.is_empty() => {
                self.query.clear();
                self.refilter();
                Action::None
            }
            KeyCode::Esc => Action::Quit,
            KeyCode::Tab => {
                self.switch_to(self.tab.step(false));
                Action::None
            }
            KeyCode::BackTab => {
                self.switch_to(self.tab.step(true));
                Action::None
            }
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
                    if let Some(why) = Self::hands_off(&s) {
                        self.message = Some(why);
                    } else if self.is_here(&s) {
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
                Some(Item::Session(s)) if Self::hands_off(&s).is_some() => {
                    app.message = Self::hands_off(&s);
                    Action::None
                }
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
                    Some(Item::Session(s)) if Self::hands_off(&s).is_some() => {
                        self.message = Self::hands_off(&s);
                        Action::None
                    }
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
            managed_by: None,
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

    fn managed_sample() -> App {
        let mut claude = session("$9", "claude-work", "/w/acme", 99);
        claude.managed_by = Some("cc-moss".into());
        App::new(vec![claude, session("$1", "api", "/w/acme", 5)], None)
    }

    #[test]
    fn managed_sessions_only_open() {
        let mut app = managed_sample();
        select(&mut app, "claude-work");
        app.handle(key(KeyCode::Right));
        assert_eq!(
            app.mode,
            Mode::Menu {
                choices: vec![Choice::Open],
                cursor: 0
            }
        );
        assert_eq!(app.message, Some(managed_hint("cc-moss")));
        assert_eq!(app.handle(key(KeyCode::Enter)), Action::Attach("$9".into()));
        // A session without the option keeps every choice.
        let mut app = managed_sample();
        select(&mut app, "api");
        app.handle(key(KeyCode::Right));
        assert!(matches!(&app.mode, Mode::Menu { choices, .. } if choices.len() == 3));
    }

    #[test]
    fn confirm_refuses_a_managed_session() {
        let mut app = managed_sample();
        select(&mut app, "claude-work");
        app.mode = Mode::Confirm { yes: true };
        assert_eq!(app.handle(key(KeyCode::Enter)), Action::None);
        assert_eq!(app.message, Some(managed_hint("cc-moss")));
    }

    #[test]
    fn rename_refuses_a_managed_session() {
        let mut app = managed_sample();
        select(&mut app, "claude-work");
        app.mode = Mode::Rename {
            text: "other".into(),
        };
        assert_eq!(app.handle(key(KeyCode::Enter)), Action::None);
        assert_eq!(app.message, Some(managed_hint("cc-moss")));
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

    fn window(session: (&str, &str), id: &str, index: u32, name: &str, at: i64) -> WindowInfo {
        WindowInfo {
            session_id: session.0.into(),
            session_name: session.1.into(),
            id: id.into(),
            index,
            name: name.into(),
            active: index == 1,
            panes: 1,
            activity_ms: at,
            path: "/w".into(),
        }
    }

    fn with_windows() -> App {
        let mut app = sample(None);
        app.set_windows(vec![
            window(("$1", "api"), "@2", 2, "logs", 5),
            window(("$1", "api"), "@1", 1, "vim", 4),
            window(("$3", "scratch"), "@7", 1, "zsh", 9),
        ]);
        app
    }

    #[test]
    fn keyboard_tab_cycles_tabs() {
        let mut app = with_windows();
        assert_eq!(app.tab, Tab::Sessions);
        app.handle(key(KeyCode::Tab));
        assert_eq!(app.tab, Tab::Windows);
        assert!(matches!(app.selected(), Some(Item::Window(_))));
        app.handle(key(KeyCode::Tab));
        assert_eq!(app.tab, Tab::Folders);
        app.handle(key(KeyCode::Tab));
        assert_eq!(app.tab, Tab::Sessions);
        app.handle(key(KeyCode::BackTab));
        assert_eq!(app.tab, Tab::Folders);
    }

    #[test]
    fn tab_switch_clears_the_search() {
        let mut app = with_windows();
        typed(&mut app, "zzz");
        app.handle(key(KeyCode::Tab));
        assert!(app.query.is_empty());
        assert_eq!(app.matched, 3);
        // → has no menu on the windows tab.
        app.handle(key(KeyCode::Right));
        assert_eq!(app.mode, Mode::Browse);
    }

    #[test]
    fn windows_are_grouped_and_searchable() {
        let mut app = with_windows();
        app.handle(key(KeyCode::Tab));
        let shown: Vec<&str> = app
            .rows()
            .iter()
            .filter_map(|r| app.window(*r).map(|w| w.name.as_str()))
            .collect();
        // Most recent session first; windows in index order.
        assert_eq!(shown, ["zsh", "vim", "logs"]);
        typed(&mut app, "logs");
        assert_eq!(app.matched, 1);
        assert_eq!(
            app.handle(key(KeyCode::Enter)),
            Action::AttachWindow {
                session: "$1".into(),
                window: "@2".into()
            }
        );
        // The session's name finds its windows too.
        let mut app = with_windows();
        app.handle(key(KeyCode::Tab));
        typed(&mut app, "scratch");
        assert_eq!(app.matched, 1);
    }

    #[test]
    fn folders_open_or_start() {
        let mut app = sample(None);
        app.set_folders(vec!["/w/acme".into(), "/w/fresh".into()]);
        app.handle(key(KeyCode::BackTab));
        assert_eq!(app.tab, Tab::Folders);
        // /w/acme has sessions: the most recent one, web ($2).
        assert_eq!(app.handle(key(KeyCode::Enter)), Action::Attach("$2".into()));
        app.handle(key(KeyCode::Down));
        assert_eq!(
            app.handle(key(KeyCode::Enter)),
            Action::NewIn("/w/fresh".into())
        );
    }

    #[test]
    fn folders_search_by_name() {
        let mut app = sample(None);
        app.set_folders(vec![
            "/private/tmp/api".into(),
            "/home/u/code/billing".into(),
            "/home/u/code/web".into(),
        ]);
        app.handle(key(KeyCode::BackTab));
        typed(&mut app, "code");
        // The parent's name counts; the rest of the path doesn't.
        assert_eq!(app.matched, 2);
        app.handle(key(KeyCode::Esc));
        typed(&mut app, "api");
        assert_eq!(app.matched, 1);
    }

    #[test]
    fn empty_server_offers_new() {
        let mut app = App::new(vec![], None);
        assert_eq!(app.rows(), [Row::New]);
        assert_eq!(app.handle(key(KeyCode::Enter)), Action::New { name: None });
    }
}
