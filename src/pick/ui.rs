//! Drawing the picker: a quiet list on the left, the selected session's
//! details on the right. Adapted from cc-moss (MIT,
//! https://github.com/larrykoo711/cc-moss), `cc-moss/src/pick/ui.rs`.
//!
//! The list carries only what tells sessions apart (a dot, a name, how long
//! ago); folders appear once, as group titles. Everything else about a
//! session is shown for the selected one only, in the card.

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph, Wrap};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

use super::app::{App, Item, Mode, Row, Tab};
use crate::i18n::Lang;
use crate::tmux::{SessionInfo, WindowInfo};

/// Secondary text.
const GREY: Color = Color::Rgb(150, 150, 150);
/// Quiet labels, rules and separators.
const FAINT: Color = Color::Rgb(110, 110, 110);
/// The selection band.
const BAND: Color = Color::Rgb(52, 52, 56);
/// Text on the band: set, never the terminal's own colour, which is dark on
/// a light terminal and would vanish on the band.
const ON_BAND: Color = Color::Rgb(236, 236, 236);
/// ta's accent: highlights, the selection bar.
const ACCENT: Color = Color::Rgb(120, 200, 120);
/// Closing; the "you are here" mark.
const RED: Color = Color::Rgb(232, 86, 86);
/// Messages in the hint line.
const AMBER: Color = Color::Rgb(240, 186, 70);
/// Below this width the card goes under the list.
pub const SPLIT_MIN_WIDTH: u16 = 90;

/// Everything the view needs besides the app state.
pub struct View<'a> {
    pub now_ms: i64,
    pub home: Option<&'a str>,
    /// Where a new session would start.
    pub cwd: &'a str,
    pub version: &'a str,
}

/// Which list row each screen line shows, for mouse clicks.
#[derive(Default)]
pub struct HitMap {
    pub lines: Vec<(u16, usize)>,
}

impl HitMap {
    pub fn row_at(&self, y: u16) -> Option<usize> {
        self.lines
            .iter()
            .find(|(line, _)| *line == y)
            .map(|(_, r)| *r)
    }
}

pub fn draw(frame: &mut Frame, app: &App, view: &View) -> HitMap {
    let area = frame.area();
    let [header, search, rule1, body, rule2, hint] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(4),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(area);

    draw_header(frame, header, app, view);
    draw_search(frame, search, app);
    rule(frame, rule1);
    let hits = if area.width >= SPLIT_MIN_WIDTH {
        let list_w = (area.width * 2 / 5).clamp(34, 48);
        let [list, gap, card] = Layout::horizontal([
            Constraint::Length(list_w),
            Constraint::Length(3),
            Constraint::Min(20),
        ])
        .areas(body);
        let divider: Vec<Line> = (0..gap.height)
            .map(|_| Line::styled(" │ ", Style::new().fg(FAINT)))
            .collect();
        frame.render_widget(Paragraph::new(divider), gap);
        let hits = draw_list(frame, list, app, view);
        draw_card(frame, card, app, view);
        hits
    } else {
        let card_h = 10.min(body.height * 3 / 5);
        let [list, sep, card] = Layout::vertical([
            Constraint::Min(2),
            Constraint::Length(1),
            Constraint::Length(card_h),
        ])
        .areas(body);
        let hits = draw_list(frame, list, app, view);
        rule(frame, sep);
        draw_card(frame, card, app, view);
        hits
    };
    rule(frame, rule2);
    draw_hint(frame, hint, app);
    hits
}

fn grey() -> Style {
    Style::new().fg(GREY)
}

fn rule(frame: &mut Frame, area: Rect) {
    frame.render_widget(
        Line::styled("─".repeat(area.width as usize), Style::new().fg(FAINT)),
        area,
    );
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App, view: &View) {
    let mut left = vec![
        Span::styled(" ta", Style::new().fg(ACCENT).add_modifier(Modifier::BOLD)),
        Span::styled(format!("  {}", view.version), Style::new().fg(FAINT)),
        Span::raw("   "),
    ];
    for tab in Tab::ALL {
        let style = if tab == app.tab {
            Style::new().fg(ACCENT).add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(FAINT)
        };
        left.push(Span::styled(format!(" {} ", tab.label()), style));
    }
    let left = Line::from(left);
    let total = app.total;
    let plural = if total == 1 { "" } else { "s" };
    // Styled span, not a styled line: a line's style covers the whole row
    // and would grey out the name on the left.
    let right = Line::from(Span::styled(
        match app.tab {
            Tab::Sessions => tr!(
                format!("{total} session{plural} "),
                format!("共 {total} 个会话 ")
            ),
            Tab::Windows => tr!(
                format!("{total} window{plural} "),
                format!("共 {total} 个窗口 ")
            ),
            Tab::Folders => tr!(
                format!("{total} folder{plural} "),
                format!("共 {total} 个目录 ")
            ),
        },
        grey(),
    ))
    .right_aligned();
    frame.render_widget(left, area);
    frame.render_widget(right, area);
}

fn draw_search(frame: &mut Frame, area: Rect, app: &App) {
    let typing = !app.query.is_empty();
    let line = if typing {
        Line::from(vec![
            Span::styled(" ⌕ ", Style::new().fg(ACCENT)),
            Span::raw(app.query.clone()),
        ])
    } else {
        Line::from(vec![
            Span::styled(" ⌕ ", grey()),
            Span::styled(
                match app.tab {
                    Tab::Sessions => tr!(
                        "Find a session, or type a name to start one",
                        "搜一个会话，或者输入名字开个新的"
                    ),
                    Tab::Windows => tr!("Find a window", "搜一个窗口"),
                    Tab::Folders => tr!("Find a folder to work in", "搜一个目录，在那里开会话"),
                },
                Style::new().fg(FAINT),
            ),
        ])
    };
    frame.render_widget(line, area);
    if typing {
        frame.render_widget(
            Line::from(Span::styled(
                tr!(
                    format!("{} of {} ", app.matched, app.total),
                    format!("{} / {} ", app.matched, app.total)
                ),
                grey(),
            ))
            .right_aligned(),
            area,
        );
    }
    if app.mode == Mode::Browse {
        let x = area.x.saturating_add(3).saturating_add(cols(&app.query));
        frame.set_cursor_position((x.min(area.right().saturating_sub(1)), area.y));
    }
}

/// One rendered list line and the row it belongs to.
struct ListLine {
    line: Line<'static>,
    row: Option<usize>,
}

fn draw_list(frame: &mut Frame, area: Rect, app: &App, view: &View) -> HitMap {
    let width = area.width as usize;
    let labels = group_labels(app, view);
    let mut lines: Vec<ListLine> = Vec::new();
    let mut cursor_line = 0;
    for (i, row) in app.rows().iter().enumerate() {
        match row {
            Row::Header(g) => {
                if !lines.is_empty() {
                    lines.push(ListLine {
                        line: Line::raw(""),
                        row: None,
                    });
                }
                lines.push(ListLine {
                    line: Line::styled(
                        format!(" {}", fit(&labels[*g], width.saturating_sub(1))),
                        Style::new().fg(GREY).add_modifier(Modifier::BOLD),
                    ),
                    row: None,
                });
            }
            Row::New => {
                let q = app.query.trim();
                let what = if q.is_empty() {
                    tr!("New session here", "在这里新建会话").to_string()
                } else {
                    tr!(format!("New session \"{q}\""), format!("新会话「{q}」"))
                };
                lines.push(ListLine {
                    line: Line::from(vec![
                        Span::styled("  + ", Style::new().fg(ACCENT).add_modifier(Modifier::BOLD)),
                        Span::styled(fit(&what, width.saturating_sub(4)), grey()),
                    ]),
                    row: Some(i),
                });
            }
            Row::Session(..) => {
                let s = app.session(*row).expect("session row");
                lines.push(ListLine {
                    line: list_row(s, app, view, width, i == app.cursor),
                    row: Some(i),
                });
            }
            Row::WindowHeader(g) => {
                if !lines.is_empty() {
                    lines.push(ListLine {
                        line: Line::raw(""),
                        row: None,
                    });
                }
                let name = &app.window_groups()[*g].session_name;
                lines.push(ListLine {
                    line: Line::styled(
                        format!(" {}", fit(name, width.saturating_sub(1))),
                        Style::new().fg(GREY).add_modifier(Modifier::BOLD),
                    ),
                    row: None,
                });
            }
            Row::Window(..) => {
                let w = app.window(*row).expect("window row");
                lines.push(ListLine {
                    line: window_row(w, app, view, width, i == app.cursor),
                    row: Some(i),
                });
            }
            Row::Folder(_) => {
                let path = app.folder(*row).expect("folder row");
                lines.push(ListLine {
                    line: folder_row(path, app, view, width, i == app.cursor),
                    row: Some(i),
                });
            }
        }
        if i == app.cursor {
            cursor_line = lines.len() - 1;
            let last = lines.last_mut().expect("just pushed");
            last.line = band(std::mem::take(&mut last.line), width);
        }
    }

    if app.rows().is_empty() && app.query.trim().is_empty() {
        let empty: &[&str] = match app.tab {
            Tab::Sessions => &[],
            Tab::Windows => &[tr!("  No windows yet.", "  还没有窗口。")],
            // Each line fits the narrowest list (34 columns).
            Tab::Folders => &[
                tr!(
                    "  No folders yet. Install zoxide,",
                    "  还没有目录。装上 zoxide，"
                ),
                tr!(
                    "  or set [folders] roots in",
                    "  或在 ~/.config/ta/config.toml"
                ),
                tr!("  ~/.config/ta/config.toml", "  里写 [folders] roots"),
            ],
        };
        for text in empty {
            lines.push(ListLine {
                line: Line::styled(fit(text, width), grey()),
                row: None,
            });
        }
    }
    let height = area.height as usize;
    let top = (cursor_line + 1).saturating_sub(height);
    let mut hits = HitMap::default();
    let shown: Vec<Line> = lines
        .into_iter()
        .skip(top)
        .take(height)
        .enumerate()
        .map(|(n, l)| {
            if let Some(r) = l.row {
                hits.lines.push((area.y + n as u16, r));
            }
            l.line
        })
        .collect();
    frame.render_widget(Paragraph::new(shown), area);
    hits
}

/// Folder names for the group titles; the full (short) path where two
/// groups share a folder name.
fn group_labels(app: &App, view: &View) -> Vec<String> {
    let base = |p: &str| {
        let t = tilde(p, view.home);
        t.rsplit('/')
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or(&t)
            .to_string()
    };
    let bases: Vec<String> = app.groups().iter().map(|g| base(&g.path)).collect();
    app.groups()
        .iter()
        .zip(&bases)
        .map(|(g, b)| {
            if bases.iter().filter(|x| *x == b).count() > 1 {
                short_path(&g.path, view.home)
            } else {
                b.clone()
            }
        })
        .collect()
}

/// Attached: a terminal shows it right now.
fn dot(s: &SessionInfo, here: bool) -> Span<'static> {
    if here {
        Span::styled("◆", Style::new().fg(RED))
    } else if s.attached {
        Span::styled("●", Style::new().fg(ACCENT))
    } else {
        Span::styled("○", Style::new().fg(FAINT))
    }
}

/// `● name ……… 3m`: one line per session.
fn list_row(
    s: &SessionInfo,
    app: &App,
    view: &View,
    width: usize,
    selected: bool,
) -> Line<'static> {
    let age = short_ago(view.now_ms - s.activity_ms);
    // Who manages it, after the name: `claude-work · cc-moss`.
    let tool = s
        .managed_by
        .as_deref()
        .map(|t| format!("  {}", fit(t, 12)))
        .unwrap_or_default();
    let room = width.saturating_sub(5 + age.width() + 2 + tool.width());
    let mut spans = vec![Span::raw("  "), dot(s, app.is_here(s)), Span::raw(" ")];
    let name_style = if selected {
        Style::new().add_modifier(Modifier::BOLD)
    } else {
        Style::new()
    };
    spans.extend(highlighted(&fit(&s.name, room), &app.query, name_style));
    if !tool.is_empty() {
        spans.push(Span::styled(tool, Style::new().fg(FAINT)));
    }
    let used: usize = spans.iter().map(|s| s.content.width()).sum();
    spans.push(Span::raw(
        " ".repeat(width.saturating_sub(used + age.width() + 1)),
    ));
    spans.push(Span::styled(age, Style::new().fg(FAINT)));
    Line::from(spans)
}

/// `● 2: logs ……… 3m`: one line per window; the dot marks its session's current window.
fn window_row(
    w: &WindowInfo,
    app: &App,
    view: &View,
    width: usize,
    selected: bool,
) -> Line<'static> {
    let age = short_ago(view.now_ms - w.activity_ms);
    let label = format!("{}: {}", w.index, w.name);
    let room = width.saturating_sub(5 + age.width() + 2);
    let dot = if w.active {
        Span::styled("●", Style::new().fg(ACCENT))
    } else {
        Span::styled("○", Style::new().fg(FAINT))
    };
    let mut spans = vec![Span::raw("  "), dot, Span::raw(" ")];
    let style = if selected {
        Style::new().add_modifier(Modifier::BOLD)
    } else {
        Style::new()
    };
    spans.extend(highlighted(&fit(&label, room), &app.query, style));
    let used: usize = spans.iter().map(|s| s.content.width()).sum();
    spans.push(Span::raw(
        " ".repeat(width.saturating_sub(used + age.width() + 1)),
    ));
    spans.push(Span::styled(age, Style::new().fg(FAINT)));
    Line::from(spans)
}

/// `● webshop  ~/code`: a folder, its parent faint; the dot is filled when a
/// session already runs there.
fn folder_row(path: &str, app: &App, view: &View, width: usize, selected: bool) -> Line<'static> {
    let (parent, name) = split_folder(path, view.home);
    let dot = if app.session_in(path).is_some() {
        Span::styled("●", Style::new().fg(ACCENT))
    } else {
        Span::styled("○", Style::new().fg(FAINT))
    };
    let style = if selected {
        Style::new().add_modifier(Modifier::BOLD)
    } else {
        Style::new()
    };
    // The parent tells same-named folders apart: keep up to a third of the
    // row for it, the rest for the name.
    let parent_room = (parent.width() + 2).min(width / 3);
    let name = fit(&name, width.saturating_sub(6 + parent_room));
    let mut spans = vec![Span::raw("  "), dot, Span::raw(" ")];
    spans.extend(highlighted(&name, &app.query, style));
    let used: usize = spans.iter().map(|s| s.content.width()).sum();
    let room = width.saturating_sub(used + 3);
    if room > 4 {
        spans.push(Span::styled(
            format!("  {}", fit(&parent, room)),
            Style::new().fg(FAINT),
        ));
    }
    Line::from(spans)
}

/// `/home/u/code/web` → (`~/code`, `web`).
fn split_folder(path: &str, home: Option<&str>) -> (String, String) {
    let t = tilde(path, home);
    match t.trim_end_matches('/').rsplit_once('/') {
        Some((parent, name)) if !name.is_empty() => (
            if parent.is_empty() {
                "/".into()
            } else {
                parent.to_string()
            },
            name.to_string(),
        ),
        _ => (String::new(), t),
    }
}

/// The name with the characters matching the search picked out.
fn highlighted(name: &str, query: &str, base: Style) -> Vec<Span<'static>> {
    let plain = || vec![Span::styled(name.to_string(), base)];
    if query.trim().is_empty() {
        return plain();
    }
    let pattern = Pattern::parse(query.trim(), CaseMatching::Ignore, Normalization::Smart);
    let mut matcher = Matcher::new(Config::DEFAULT);
    let mut buf = Vec::new();
    let mut idx = Vec::new();
    if pattern
        .indices(Utf32Str::new(name, &mut buf), &mut matcher, &mut idx)
        .is_none()
    {
        return plain();
    }
    name.chars()
        .enumerate()
        .map(|(i, ch)| {
            let style = if idx.contains(&(i as u32)) {
                base.fg(ACCENT).add_modifier(Modifier::BOLD)
            } else {
                base
            };
            Span::styled(ch.to_string(), style)
        })
        .collect()
}

/// The selection: an accent bar and a soft band across the list column.
fn band(line: Line<'static>, width: usize) -> Line<'static> {
    let mut spans: Vec<Span> = line.spans;
    if let Some(first) = spans.first_mut() {
        if first.content.starts_with(' ') {
            let rest: String = first.content.chars().skip(1).collect();
            *first = Span::raw(rest);
            spans.insert(0, Span::styled("▌", Style::new().fg(ACCENT)));
        }
    }
    let used: usize = spans.iter().map(|s| s.content.width()).sum();
    spans.push(Span::raw(" ".repeat(width.saturating_sub(used))));
    Line::from(
        spans
            .into_iter()
            .map(|s| {
                let mut style = s.style.bg(BAND);
                if style.fg.is_none() {
                    style = style.fg(ON_BAND);
                }
                s.style(style)
            })
            .collect::<Vec<_>>(),
    )
}

/// The detail card for the selected item.
fn draw_card(frame: &mut Frame, area: Rect, app: &App, view: &View) {
    let Some(item) = app.selected() else {
        return;
    };
    let label = |t: &str| {
        Line::styled(
            t.to_uppercase(),
            Style::new().fg(FAINT).add_modifier(Modifier::BOLD),
        )
    };
    let key = |k: &str| Span::styled(k.to_string(), Style::new().add_modifier(Modifier::BOLD));
    let word = |w: &str| Span::styled(w.to_string(), grey());
    let mut lines: Vec<Line> = Vec::new();
    let title = match &item {
        Item::New => {
            let q = app.query.trim();
            lines.push(Line::styled(
                if q.is_empty() {
                    tr!("New session", "新会话").to_string()
                } else {
                    tr!(format!("New session \"{q}\""), format!("新会话「{q}」"))
                },
                Style::new().add_modifier(Modifier::BOLD),
            ));
            lines.push(Line::styled(
                tr!(
                    "A fresh shell in a tmux session of its own.",
                    "一个全新的 shell，在它自己的 tmux 会话里。"
                ),
                grey(),
            ));
            lines.push(Line::raw(""));
            lines.push(label(tr!("Folder", "目录")));
            lines.push(Line::raw(tilde(view.cwd, view.home)));
            lines.push(Line::raw(""));
            lines.push(Line::from(vec![key("↵"), word(tr!(" Start", " 新建"))]));
            String::new()
        }
        Item::Session(s) => {
            let here = app.is_here(s);
            lines.push(Line::styled(
                s.name.clone(),
                Style::new().add_modifier(Modifier::BOLD),
            ));
            let (state, color) = if here {
                (tr!("You are here", "你在这里"), RED)
            } else if s.attached {
                (tr!("Open in a terminal", "有终端接着"), ACCENT)
            } else {
                (tr!("Not open anywhere", "没有终端接着"), GREY)
            };
            let windows = tr!(
                format!(
                    "{} window{}",
                    s.windows,
                    if s.windows == 1 { "" } else { "s" }
                ),
                format!("{} 个窗口", s.windows)
            );
            lines.push(Line::from(vec![
                dot(s, here),
                Span::styled(format!(" {state}"), Style::new().fg(color)),
                Span::styled(
                    format!(
                        "  ·  {windows}  ·  {}",
                        long_ago(view.now_ms - s.activity_ms)
                    ),
                    grey(),
                ),
            ]));
            if let Some(tool) = &s.managed_by {
                lines.push(Line::styled(
                    super::app::managed_hint(tool),
                    Style::new().fg(AMBER),
                ));
            }
            lines.push(Line::raw(""));
            lines.push(label(tr!("Folder", "目录")));
            lines.push(Line::raw(tilde(&s.path, view.home)));
            lines.push(Line::raw(""));
            // An open menu, question or rename box takes this line's place.
            if matches!(app.mode, Mode::Browse) {
                lines.push(Line::from(vec![
                    key("↵"),
                    word(tr!(" Open", " 进入")),
                    Span::raw("     "),
                    key("→"),
                    word(tr!(" More", " 更多")),
                ]));
            }
            s.name.clone()
        }
        Item::Window(w) => {
            lines.push(Line::styled(
                format!("{}: {}", w.index, w.name),
                Style::new().add_modifier(Modifier::BOLD),
            ));
            let panes = tr!(
                format!("{} pane{}", w.panes, if w.panes == 1 { "" } else { "s" }),
                format!("{} 个窗格", w.panes)
            );
            lines.push(Line::styled(
                format!(
                    "{}  ·  {panes}  ·  {}",
                    w.session_name,
                    long_ago(view.now_ms - w.activity_ms)
                ),
                grey(),
            ));
            lines.push(Line::raw(""));
            lines.push(label(tr!("Folder", "目录")));
            lines.push(Line::raw(tilde(&w.path, view.home)));
            lines.push(Line::raw(""));
            lines.push(Line::from(vec![key("↵"), word(tr!(" Go there", " 进入"))]));
            String::new()
        }
        Item::Folder { path, session } => {
            let (_, name) = split_folder(path, view.home);
            lines.push(Line::styled(
                name,
                Style::new().add_modifier(Modifier::BOLD),
            ));
            lines.push(match session {
                Some(s) => Line::from(vec![
                    Span::styled("● ", Style::new().fg(ACCENT)),
                    Span::styled(
                        tr!(
                            format!("{} is open here", s.name),
                            format!("{} 已经在这里", s.name)
                        ),
                        Style::new().fg(ACCENT),
                    ),
                ]),
                None => Line::styled(tr!("No session here yet", "这里还没有会话"), grey()),
            });
            lines.push(Line::raw(""));
            lines.push(label(tr!("Folder", "目录")));
            lines.push(Line::raw(tilde(path, view.home)));
            lines.push(Line::raw(""));
            let act = if session.is_some() {
                tr!(" Go there", " 进入")
            } else {
                tr!(" Start a session here", " 在这里新建会话")
            };
            lines.push(Line::from(vec![key("↵"), word(act)]));
            String::new()
        }
    };
    let inner = Rect::new(
        area.x + 1,
        area.y,
        area.width.saturating_sub(2),
        area.height,
    );
    let card = Paragraph::new(lines).wrap(Wrap { trim: false });
    // Rows the card's text takes once wrapped: the box opens right below it.
    let used = u16::try_from(card.line_count(inner.width)).unwrap_or(u16::MAX);
    frame.render_widget(card, inner);
    draw_overlay(frame, inner, used, app, &title);
}

/// Menu, confirmation and rename boxes, right below the card's text (`used`
/// rows); pushed up only when the terminal is too short to fit them there.
fn draw_overlay(frame: &mut Frame, card: Rect, used: u16, app: &App, title: &str) {
    let place = |h: u16| {
        let w = card.width.min(48);
        let h = h.min(card.height);
        let below = card.y.saturating_add(used);
        let y = below.min(card.bottom().saturating_sub(h));
        Rect::new(card.x, y, w, h)
    };
    let boxed = |t: String| {
        Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(ACCENT))
            .title(Span::styled(
                format!(" {t} "),
                Style::new().add_modifier(Modifier::BOLD),
            ))
    };
    match &app.mode {
        Mode::Browse => {}
        Mode::Menu { choices, cursor } => {
            let area = place(choices.len() as u16 + 2);
            let lines: Vec<Line> = choices
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    let base = if *c == super::app::Choice::Close {
                        Style::new().fg(RED)
                    } else {
                        Style::new()
                    };
                    if i == *cursor {
                        Line::from(vec![
                            Span::styled(" ❯ ", Style::new().fg(ACCENT)),
                            Span::styled(c.label(), base.add_modifier(Modifier::BOLD)),
                        ])
                        .style(Style::new().bg(BAND).fg(ON_BAND))
                    } else {
                        Line::from(vec![Span::raw("   "), Span::styled(c.label(), base)])
                    }
                })
                .collect();
            frame.render_widget(Clear, area);
            frame.render_widget(Paragraph::new(lines).block(boxed(title.to_string())), area);
        }
        Mode::Confirm { yes } => {
            let button = |text: &str, on: bool, danger: bool| {
                let style = match (on, danger) {
                    (true, true) => Style::new()
                        .fg(ON_BAND)
                        .bg(RED)
                        .add_modifier(Modifier::BOLD),
                    (true, false) => Style::new()
                        .fg(ON_BAND)
                        .bg(BAND)
                        .add_modifier(Modifier::BOLD),
                    (false, _) => grey(),
                };
                Span::styled(format!(" {text} "), style)
            };
            let lines = vec![
                Line::styled(
                    tr!(
                        " Ends it and everything running in it. No undo.",
                        " 会结束这个会话和里面运行的一切，没法撤销。"
                    ),
                    grey(),
                ),
                Line::raw(""),
                Line::from(vec![
                    Span::raw(" "),
                    button(tr!("Close it", "关掉它"), *yes, true),
                    Span::raw("   "),
                    button(tr!("Keep it", "再想想"), !*yes, false),
                ]),
            ];
            let box_ = Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .block(boxed(tr!(
                    format!("Close {title}?"),
                    format!("关闭 {title}？")
                )));
            // As tall as the wrapped text, so the buttons always show.
            let inner_w = card.width.min(48).saturating_sub(2);
            let h = u16::try_from(box_.line_count(inner_w)).unwrap_or(u16::MAX);
            let area = place(h);
            frame.render_widget(Clear, area);
            frame.render_widget(box_, area);
        }
        Mode::Rename { text } => {
            let area = place(3);
            let inner = Block::bordered().inner(area);
            frame.render_widget(Clear, area);
            frame.render_widget(
                Paragraph::new(Line::raw(format!(" {text}")))
                    .block(boxed(tr!("New name", "新名字").into())),
                area,
            );
            let x = inner.x.saturating_add(1).saturating_add(cols(text));
            frame.set_cursor_position((x.min(inner.right().saturating_sub(1)), inner.y));
        }
    }
}

/// Terminal columns of `text`, capped at what a cursor position can hold.
fn cols(text: &str) -> u16 {
    u16::try_from(text.width()).unwrap_or(u16::MAX)
}

fn draw_hint(frame: &mut Frame, area: Rect, app: &App) {
    let key = |k: &str| Span::styled(k.to_string(), Style::new().add_modifier(Modifier::BOLD));
    let word = |w: &str| Span::styled(w.to_string(), grey());
    let gap = || Span::raw("    ");
    let parts: Vec<Span> = if let Some(m) = &app.message {
        vec![Span::styled(format!(" {m}"), Style::new().fg(AMBER))]
    } else {
        match &app.mode {
            Mode::Browse => {
                let open = match app.selected() {
                    Some(Item::New | Item::Folder { session: None, .. }) => {
                        tr!(" start", " 新建")
                    }
                    _ => tr!(" open", " 进入"),
                };
                let esc = if app.query.is_empty() {
                    tr!(" quit", " 退出")
                } else {
                    tr!(" clear", " 清空")
                };
                let mut parts = vec![
                    Span::raw(" "),
                    key("↑↓"),
                    word(tr!(" move", " 移动")),
                    gap(),
                    key("↵"),
                    word(open),
                    gap(),
                ];
                if app.tab == Tab::Sessions {
                    parts.extend([key("→"), word(tr!(" more", " 更多")), gap()]);
                }
                parts.extend([
                    key("tab"),
                    word(tr!(" switch", " 切页")),
                    gap(),
                    key("esc"),
                    word(esc),
                ]);
                parts
            }
            Mode::Menu { .. } => vec![
                Span::raw(" "),
                key("↑↓"),
                word(tr!(" choose", " 选择")),
                gap(),
                key("↵"),
                word(tr!(" confirm", " 确认")),
                gap(),
                key("←"),
                word(tr!(" back", " 返回")),
            ],
            Mode::Confirm { .. } => vec![
                Span::raw(" "),
                key("←→"),
                word(tr!(" choose", " 选择")),
                gap(),
                key("↵"),
                word(tr!(" confirm", " 确认")),
                gap(),
                key("esc"),
                word(tr!(" cancel", " 取消")),
            ],
            Mode::Rename { .. } => vec![
                Span::raw(" "),
                key("↵"),
                word(tr!(" save", " 保存")),
                gap(),
                key("esc"),
                word(tr!(" cancel", " 取消")),
            ],
        }
    };
    frame.render_widget(Line::from(parts), area);
}

/// `text` cut with `…` to at most `w` display columns.
fn fit(text: &str, w: usize) -> String {
    if text.width() <= w {
        return text.to_string();
    }
    let mut out = String::new();
    for ch in text.chars() {
        if out.width() + unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0) + 1 > w {
            break;
        }
        out.push(ch);
    }
    out.push('…');
    out
}

fn tilde(path: &str, home: Option<&str>) -> String {
    match home
        .filter(|h| !h.is_empty())
        .and_then(|h| path.strip_prefix(h))
    {
        Some(rest) if rest.is_empty() || rest.starts_with('/') => format!("~{rest}"),
        _ => path.to_string(),
    }
}

/// `…/parent/last` when the path is long.
fn short_path(path: &str, home: Option<&str>) -> String {
    let t = tilde(path, home);
    if t.width() <= 28 {
        return t;
    }
    let mut parts = t.rsplit('/');
    let last = parts.next().unwrap_or(&t);
    let parent = parts.next().unwrap_or("");
    format!("…/{parent}/{last}")
}

/// `now`, `3m`, `2h`, `5d`: for the list.
fn short_ago(ms: i64) -> String {
    let s = ms.max(0) / 1000;
    match (s, crate::i18n::lang()) {
        (0..=59, Lang::En) => "now".into(),
        (0..=59, Lang::Zh) => "刚刚".into(),
        (60..=3599, Lang::En) => format!("{}m", s / 60),
        (60..=3599, Lang::Zh) => format!("{}分钟", s / 60),
        (3600..=86_399, Lang::En) => format!("{}h", s / 3600),
        (3600..=86_399, Lang::Zh) => format!("{}小时", s / 3600),
        (_, Lang::En) => format!("{}d", s / 86_400),
        (_, Lang::Zh) => format!("{}天", s / 86_400),
    }
}

/// `just now`, `3 minutes ago`: for the card.
fn long_ago(ms: i64) -> String {
    let s = ms.max(0) / 1000;
    let unit = |n: i64, en: &str, zh: &str| {
        tr!(
            format!("{n} {en}{} ago", if n == 1 { "" } else { "s" }),
            format!("{n} {zh}前")
        )
    };
    match s {
        0..=59 => tr!("just now", "刚刚").into(),
        60..=3599 => unit(s / 60, "minute", "分钟"),
        3600..=86_399 => unit(s / 3600, "hour", "小时"),
        _ => unit(s / 86_400, "day", "天"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn session(id: &str, name: &str, path: &str, at: i64, attached: bool) -> SessionInfo {
        SessionInfo {
            managed_by: None,
            id: id.into(),
            name: name.into(),
            windows: 2,
            activity_ms: at,
            attached,
            path: path.into(),
        }
    }

    fn sample() -> App {
        App::new(
            vec![
                session("$1", "api-gateway", "/home/u/acme", 540_000, true),
                session("$2", "接口评审", "/home/u/acme", 300_000, false),
                session(
                    "$3",
                    "a-very-long-session-name-that-will-not-fit-anywhere",
                    "/home/u/billing",
                    0,
                    false,
                ),
            ],
            Some("$2".into()),
        )
    }

    fn render(app: &App, w: u16, h: u16) -> String {
        let view = View {
            now_ms: 600_000,
            home: Some("/home/u"),
            cwd: "/home/u/projects",
            version: "0.1.0",
        };
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| {
            draw(f, app, &view);
        })
        .unwrap();
        let buf = t.backend().buffer().clone();
        // A wide character fills two cells; read the second as nothing.
        (0..h)
            .map(|y| {
                let mut line = String::new();
                let mut x = 0;
                while x < w {
                    let sym = buf[(x, y)].symbol();
                    line.push_str(sym);
                    x += sym.width().max(1) as u16;
                }
                line
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn buffer(app: &App, w: u16, h: u16) -> ratatui::buffer::Buffer {
        let view = View {
            now_ms: 600_000,
            home: Some("/home/u"),
            cwd: "/home/u/projects",
            version: "0.1.0",
        };
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| {
            draw(f, app, &view);
        })
        .unwrap();
        t.backend().buffer().clone()
    }

    #[test]
    fn right_aligned_text_keeps_the_colours_on_its_left() {
        crate::i18n::set_for_test(Lang::En);
        let mut app = sample();
        let buf = buffer(&app, 100, 20);
        assert_eq!(buf[(1, 0)].symbol(), "t");
        assert_eq!(buf[(1, 0)].fg, ACCENT, "the name in the header");
        app.handle(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        let buf = buffer(&app, 100, 20);
        assert_eq!(buf[(3, 1)].symbol(), "a");
        assert_ne!(
            buf[(3, 1)].fg,
            GREY,
            "what you type stays in the text colour"
        );
    }

    #[test]
    fn the_selection_band_sets_its_own_text_colour() {
        crate::i18n::set_for_test(Lang::En);
        let app = sample();
        let buf = buffer(&app, 100, 20);
        // The selected row: find the band, then its name.
        let y = (0..20).find(|&y| buf[(0, y)].symbol() == "▌").unwrap();
        let name_x = (0..40).find(|&x| buf[(x, y)].symbol() == "a").unwrap();
        assert_eq!(buf[(name_x, y)].bg, BAND);
        assert_eq!(buf[(name_x, y)].fg, ON_BAND);
    }

    fn with_folders() -> App {
        let mut app = sample();
        app.set_folders(vec![
            "/home/u/acme".into(),
            "/home/u/code/a-folder-with-a-name-far-too-long-for-any-list-at-all".into(),
            "/srv/接口".into(),
        ]);
        app.handle(KeyEvent::new(KeyCode::BackTab, KeyModifiers::NONE));
        app
    }

    #[test]
    fn folders_tab_fits_every_width() {
        crate::i18n::set_for_test(Lang::Zh);
        for w in [120, 90, 80, 60] {
            let text = render(&with_folders(), w, 24);
            for line in text.lines() {
                assert!(line.width() <= w as usize, "{w}: {line:?}");
            }
            assert!(text.contains(" 目录 "), "{w}\n{text}");
            assert!(text.contains("接口"), "{w}\n{text}");
            assert!(text.contains("共 3 个目录"), "{w}\n{text}");
        }
        crate::i18n::set_for_test(Lang::En);
        let text = render(&with_folders(), 120, 24);
        // ~/acme has a session: the card says which.
        assert!(text.contains("api-gateway is open here"), "{text}");
        assert!(text.contains("↵ open"), "{text}");
        let mut app = with_folders();
        app.handle(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        let text = render(&app, 120, 24);
        assert!(text.contains("No session here yet"), "{text}");
        assert!(text.contains("↵ start"), "{text}");
        assert!(text.contains("~/code"), "{text}");
    }

    #[test]
    fn empty_folders_tab_says_how_to_add_some() {
        crate::i18n::set_for_test(Lang::En);
        let mut app = App::new(vec![], None);
        app.handle(KeyEvent::new(KeyCode::BackTab, KeyModifiers::NONE));
        let text = render(&app, 100, 20);
        assert!(text.contains("No folders yet."), "{text}");
        assert!(text.contains("[folders] roots"), "{text}");
    }

    fn with_windows() -> App {
        let mut app = sample();
        let w = |sid: &str, sname: &str, id: &str, index, name: &str| WindowInfo {
            session_id: sid.into(),
            session_name: sname.into(),
            id: id.into(),
            index,
            name: name.into(),
            active: index == 1,
            panes: 2,
            activity_ms: 300_000,
            path: "/home/u/acme".into(),
        };
        app.set_windows(vec![
            w("$1", "api-gateway", "@1", 1, "vim"),
            w(
                "$1",
                "api-gateway",
                "@2",
                2,
                "a-window-with-a-name-far-too-long-for-any-list",
            ),
            w("$2", "接口评审", "@3", 1, "日志"),
        ]);
        app.handle(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        app
    }

    #[test]
    fn windows_tab_fits_every_width() {
        crate::i18n::set_for_test(Lang::Zh);
        for w in [120, 90, 80, 60] {
            let text = render(&with_windows(), w, 24);
            for line in text.lines() {
                assert!(line.width() <= w as usize, "{w}: {line:?}");
            }
            assert!(text.contains(" 窗口 "), "{w}\n{text}");
            assert!(text.contains("1: vim"), "{w}\n{text}");
            assert!(text.contains("1: 日志"), "{w}\n{text}");
            assert!(text.contains("共 3 个窗口"), "{w}\n{text}");
        }
        crate::i18n::set_for_test(Lang::En);
        let text = render(&with_windows(), 120, 24);
        assert!(text.contains("api-gateway  ·  2 panes"), "{text}");
        assert!(!text.contains("→ more"), "{text}");
        assert!(text.contains("tab switch"), "{text}");
    }

    #[test]
    fn empty_windows_tab_says_so() {
        crate::i18n::set_for_test(Lang::En);
        let mut app = App::new(vec![], None);
        app.handle(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        let text = render(&app, 100, 20);
        assert!(text.contains("No windows yet."), "{text}");
        assert!(text.contains("Find a window"), "{text}");
    }

    #[test]
    fn managed_rows_fit_every_width() {
        crate::i18n::set_for_test(Lang::Zh);
        let mut long = session(
            "$7",
            "a-very-long-claude-session-name-that-keeps-going",
            "/home/u/acme",
            500_000,
            false,
        );
        long.managed_by = Some("cc-moss".into());
        let mut app = App::new(
            vec![long, session("$1", "api", "/home/u/acme", 1, false)],
            None,
        );
        for w in [120, 90, 80, 60] {
            let text = render(&app, w, 24);
            for line in text.lines() {
                assert!(line.width() <= w as usize, "{w}: {line:?}");
            }
            assert!(text.contains("cc-moss"), "{w}\n{text}");
        }
        crate::i18n::set_for_test(Lang::En);
        app.set_sessions(app.groups()[0].sessions.clone());
        let text = render(&app, 120, 24);
        assert!(text.contains("cc-moss manages this session"), "{text}");
    }

    #[test]
    fn layout_fits_every_width() {
        crate::i18n::set_for_test(Lang::Zh);
        for w in [120, 90, 80, 60] {
            let text = render(&sample(), w, 24);
            for line in text.lines() {
                assert!(line.width() <= w as usize, "{w}: {line:?}");
            }
            assert!(text.contains("api-gateway"), "{w}\n{text}");
            assert!(text.contains("接口评审"), "{w}\n{text}");
            assert!(text.contains("acme"), "{w}\n{text}");
            // At 80 columns the list takes the full width and the name fits.
            if w != 80 {
                assert!(text.contains("…"), "long names are cut: {w}\n{text}");
            }
        }
        // Side by side from 90 columns, stacked below.
        assert!(render(&sample(), 90, 24).contains(" │ "));
        assert!(!render(&sample(), 80, 24).contains(" │ "));
    }

    #[test]
    fn card_shows_the_selected_session() {
        crate::i18n::set_for_test(Lang::En);
        let text = render(&sample(), 120, 24);
        assert!(text.contains("Open in a terminal"), "{text}");
        assert!(text.contains("2 windows"), "{text}");
        assert!(text.contains("~/acme"), "{text}");
        assert!(text.contains("1 minute ago"), "{text}");
    }

    #[test]
    fn empty_server_offers_new() {
        crate::i18n::set_for_test(Lang::En);
        let app = App::new(vec![], None);
        let text = render(&app, 100, 20);
        assert!(text.contains("New session here"), "{text}");
        assert!(text.contains("~/projects"), "{text}");
        assert!(text.contains("0 sessions"), "{text}");
    }

    #[test]
    fn action_errors_show_in_the_hint_line() {
        crate::i18n::set_for_test(Lang::En);
        let mut app = sample();
        app.message = Some("tmux kill-session failed: no such session".into());
        let text = render(&app, 100, 20);
        let last = text.lines().last().unwrap();
        assert!(last.contains("no such session"), "{last}");
    }

    #[test]
    fn confirm_box_shows_both_buttons_with_keep_selected() {
        crate::i18n::set_for_test(Lang::En);
        let mut app = sample();
        app.handle(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        app.handle(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        app.handle(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        app.handle(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let text = render(&app, 120, 24);
        assert!(text.contains("Close api-gateway?"), "{text}");
        assert!(
            text.contains("Close it") && text.contains("Keep it"),
            "{text}"
        );
    }

    #[test]
    fn the_current_session_is_marked() {
        crate::i18n::set_for_test(Lang::En);
        let mut app = sample();
        app.handle(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        let text = render(&app, 120, 24);
        assert!(text.contains("◆"), "{text}");
        assert!(text.contains("You are here"), "{text}");
    }
}
