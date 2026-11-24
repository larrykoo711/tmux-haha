//! `ta` with no arguments: a full-screen session picker. Adapted from
//! cc-moss (MIT, https://github.com/larrykoo711/cc-moss), `cc-moss/src/pick/mod.rs`.

pub mod app;
pub mod ui;

use std::io;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind, MouseButton, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::naming;
use crate::tmux::{ProcessRunner, Socket, Tmux};
use app::{Action, App};

/// How often to look for keys.
const TICK: Duration = Duration::from_millis(200);
/// Pick up new, renamed and ended sessions without asking.
const REFRESH: Duration = Duration::from_secs(2);
/// Tail lines of the selected pane shown in the detail card.
const PREVIEW_LINES: usize = 8;

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// Open the picker; then go where it was asked to.
pub fn run(
    tmux: &Tmux<ProcessRunner>,
    socket: &Socket,
    host: Option<&str>,
    local_prefix: Option<&str>,
    roots: &[String],
) -> Result<()> {
    let here = tmux.current_session(
        std::env::var("TMUX").ok().as_deref(),
        std::env::var("TMUX_PANE").ok().as_deref(),
    );
    let mut app = App::new(tmux.sessions(), here);
    app.set_host_prefix(local_prefix.map(String::from));
    app.set_windows(tmux.windows());
    app.set_folders(crate::folders::candidates(crate::folders::zoxide(), roots));
    let home = std::env::var("HOME").ok();
    let cwd = crate::cwd()?;
    let mut view = ui::View {
        now_ms: now_ms(),
        home: home.as_deref(),
        cwd: &cwd,
        version: env!("CARGO_PKG_VERSION"),
        preview: &[],
        tick: 0,
    };
    let action = {
        let _screen = Screen::enter()?;
        let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
        event_loop(&mut terminal, &mut app, tmux, &mut view)?
    };
    // The screen is restored: hand the terminal to whatever comes next.
    match action {
        Action::Attach(id) => crate::enter::enter(tmux, socket, &id),
        Action::NewIn(folder) => {
            let desired = naming::new_session_name(None, &folder, host);
            let (id, _) = crate::create(tmux, &desired, &folder)?;
            crate::enter::enter(tmux, socket, &id)
        }
        Action::AttachWindow { session, window } => {
            crate::enter::enter_window(tmux, socket, &session, &window)
        }
        Action::New { name } => {
            let desired = naming::new_session_name(name.as_deref(), &cwd, host);
            let (id, _) = crate::create(tmux, &desired, &cwd)?;
            crate::enter::enter(tmux, socket, &id)
        }
        _ => Ok(()),
    }
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    tmux: &Tmux<ProcessRunner>,
    view: &mut ui::View,
) -> Result<Action> {
    let mut last_refresh = Instant::now();
    let mut hits = ui::HitMap::default();
    let mut preview_target: Option<String> = None;
    let mut preview_lines: Vec<String> = Vec::new();
    let mut tick: usize = 0;
    loop {
        let want_target = app.selected_preview_target();
        if want_target != preview_target {
            preview_lines = want_target
                .as_deref()
                .map(|id| tmux.capture_preview(id, PREVIEW_LINES))
                .unwrap_or_default();
            preview_target = want_target;
        }
        view.now_ms = now_ms();
        view.tick = tick;
        let frame_view = ui::View {
            preview: &preview_lines,
            ..*view
        };
        terminal.draw(|f| hits = ui::draw(f, app, &frame_view))?;
        if last_refresh.elapsed() >= REFRESH {
            app.set_sessions(tmux.sessions());
            app.set_windows(tmux.windows());
            preview_target = app.selected_preview_target();
            preview_lines = preview_target
                .as_deref()
                .map(|id| tmux.capture_preview(id, PREVIEW_LINES))
                .unwrap_or_default();
            last_refresh = Instant::now();
        }
        if !event::poll(TICK)? {
            tick = tick.wrapping_add(1);
            continue;
        }
        let action = match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => app.handle(key),
            Event::Mouse(m) => match m.kind {
                MouseEventKind::Down(MouseButton::Left) => match hits.row_at(m.row) {
                    Some(row) if app.click(row) => app.open(),
                    _ => Action::None,
                },
                MouseEventKind::ScrollDown => {
                    app.scroll(true);
                    Action::None
                }
                MouseEventKind::ScrollUp => {
                    app.scroll(false);
                    Action::None
                }
                _ => Action::None,
            },
            _ => Action::None,
        };
        match action {
            Action::None => {}
            Action::Close(id) => {
                let name = name_of(tmux, &id);
                app.message = Some(match tmux.kill_session(&id) {
                    Ok(()) => tr!(format!("Closed {name}."), format!("已关闭 {name}。")),
                    Err(e) => tr!(
                        format!("Could not close {name}: {e:#}"),
                        format!("没能关闭 {name}：{e:#}")
                    ),
                });
                app.set_sessions(tmux.sessions());
                app.set_windows(tmux.windows());
                last_refresh = Instant::now();
            }
            Action::Rename { id, name } => {
                app.message = Some(rename(tmux, &id, &name));
                app.set_sessions(tmux.sessions());
                app.set_windows(tmux.windows());
                last_refresh = Instant::now();
            }
            done => return Ok(done),
        }
    }
}

fn name_of(tmux: &Tmux<ProcessRunner>, id: &str) -> String {
    tmux.sessions()
        .into_iter()
        .find(|s| s.id == id)
        .map_or_else(|| id.to_string(), |s| s.name)
}

/// Rename `$N` to `wanted`; what the hint line says about it.
fn rename(tmux: &Tmux<ProcessRunner>, id: &str, wanted: &str) -> String {
    match crate::commands::rename_session(tmux, id, wanted) {
        Ok(name) => tr!(format!("Renamed to {name}."), format!("已改名为 {name}。")),
        Err(e) => tr!(
            format!("Could not rename it: {e:#}"),
            format!("没能改名：{e:#}")
        ),
    }
}

struct Screen;

impl Screen {
    fn enter() -> Result<Self> {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            Screen::leave();
            previous(info);
        }));
        enable_raw_mode().context("cannot use this terminal")?;
        execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;
        Ok(Screen)
    }

    fn leave() {
        let _ = execute!(io::stdout(), DisableMouseCapture, LeaveAlternateScreen);
        let _ = disable_raw_mode();
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        Screen::leave();
    }
}
