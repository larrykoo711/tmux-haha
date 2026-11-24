//! The picker, driven through a real terminal: `ta` runs in a pane on a
//! private tmux server and gets its keys from `tmux send-keys`.

mod common;

use common::{wait_for, World};

#[test]
fn picker_lists_sessions() {
    let w = World::new();
    let work = w.dir("work");
    w.ta(&["-d", "api"], &work);
    w.ta(&["-d", "notes"], &w.dir("docs"));
    w.start_picker(&work);
    let screen = w.wait_for_screen(|s| s.contains("api") && s.contains("notes"));
    assert!(screen.contains("3 sessions"), "{screen}");
    assert!(
        screen.contains("work") && screen.contains("docs"),
        "{screen}"
    );
}

#[test]
fn picker_closes_the_selected_session() {
    let w = World::new();
    let work = w.dir("work");
    w.ta(&["-d", "api"], &work);
    w.ta(&["-d", "api-v2"], &work);
    w.start_picker(&work);
    w.type_text("api-v2");
    w.keys(&["Right", "Down", "Down", "Enter"]);
    w.wait_for_screen(|s| s.contains("Close api-v2?"));
    // Keep is the default: move to "Close it" first.
    w.keys(&["Left", "Enter"]);
    wait_for(|| (!w.session_names().contains(&"api-v2".to_string())).then_some(()));
    assert_eq!(w.session_names(), ["api", "driver"]);
    w.wait_for_screen(|s| s.contains("Closed api-v2."));
}

#[test]
fn picker_keeps_the_session_when_the_question_is_just_answered() {
    let w = World::new();
    let work = w.dir("work");
    w.ta(&["-d", "api"], &work);
    w.start_picker(&work);
    w.type_text("api");
    w.keys(&["Right", "Down", "Down", "Enter"]);
    w.wait_for_screen(|s| s.contains("Close api?"));
    w.keys(&["Enter"]);
    w.wait_for_screen(|s| !s.contains("Close api?"));
    assert_eq!(w.session_names(), ["api", "driver"]);
}

#[test]
fn picker_renames_a_session() {
    let w = World::new();
    let work = w.dir("work");
    w.ta(&["-d", "api"], &work);
    w.start_picker(&work);
    w.type_text("api");
    w.keys(&["Right", "Down", "Enter"]);
    w.wait_for_screen(|s| s.contains("New name"));
    w.keys(&["BSpace", "BSpace", "BSpace"]);
    w.type_text("gateway v2.1");
    w.keys(&["Enter"]);
    wait_for(|| {
        w.session_names()
            .contains(&"gateway-v2-1".to_string())
            .then_some(())
    });
    assert_eq!(w.session_names(), ["driver", "gateway-v2-1"]);
}

#[test]
fn picker_cannot_close_its_own_session() {
    let w = World::new();
    let work = w.dir("work");
    w.start_picker(&work);
    w.type_text("driver");
    w.keys(&["Right"]);
    let screen = w.wait_for_screen(|s| s.contains("can't be closed from here"));
    assert!(screen.contains("Rename"), "{screen}");
    assert!(!screen.contains("Close"), "{screen}");
    // Back out of the menu and the search; the picker (and so its session) stays.
    w.keys(&["Escape", "Escape"]);
    w.wait_for_screen(|s| s.contains("Find a session"));
    assert_eq!(w.session_names(), ["driver"]);
}

#[test]
fn picker_goes_to_the_chosen_window() {
    let w = World::new();
    let work = w.dir("work");
    w.ta(&["-d", "multi"], &work);
    w.tmux(&["new-window", "-d", "-t", "multi", "-n", "second"]);
    let current = || w.tmux(&["display-message", "-p", "-t", "multi", "#{window_name}"]);
    assert_ne!(current().trim(), "second");
    w.start_picker(&work);
    w.keys(&["Tab"]);
    w.wait_for_screen(|s| s.contains("Find a window"));
    w.type_text("second");
    w.keys(&["Enter"]);
    wait_for(|| (current().trim() == "second").then_some(()));
}

#[test]
fn picker_starts_a_session_in_the_chosen_folder() {
    let w = World::new();
    let work = w.dir("work");
    let beta = w.dir("code/beta");
    w.dir("code/alpha");
    w.config("[folders]\nroots = [\"~/code\"]\n");
    w.start_picker(&work);
    w.keys(&["BTab"]);
    w.wait_for_screen(|s| s.contains("alpha") && s.contains("beta"));
    w.type_text("beta");
    w.keys(&["Enter"]);
    wait_for(|| {
        w.session_names()
            .contains(&"beta".to_string())
            .then_some(())
    });
    let path = w.tmux(&["display-message", "-p", "-t", "beta", "#{session_path}"]);
    assert_eq!(path.trim(), beta.to_string_lossy());
}

#[test]
fn picker_folder_with_a_session_goes_there() {
    let w = World::new();
    let work = w.dir("work");
    let alpha = w.dir("code/alpha");
    w.ta(&["-d", "already"], &alpha);
    w.config("[folders]\nroots = [\"~/code\"]\n");
    w.start_picker(&work);
    w.keys(&["BTab"]);
    w.wait_for_screen(|s| s.contains("already is open here"));
    w.keys(&["Enter"]);
    // The picker leaves (its session with it); no session was started.
    wait_for(|| (!w.session_names().contains(&"driver".to_string())).then_some(()));
    assert_eq!(w.session_names(), ["already"]);
}

#[test]
fn picker_leaves_a_managed_session_alone() {
    let w = World::new();
    let work = w.dir("work");
    w.ta(&["-d", "claude-work"], &work);
    w.ta(&["-d", "plain"], &work);
    w.tmux(&["set-option", "-t", "claude-work", "@managed-by", "cc-moss"]);
    w.start_picker(&work);
    w.type_text("claude-work");
    w.keys(&["Right"]);
    let screen = w.wait_for_screen(|s| s.contains("cc-moss manages this session"));
    assert!(
        !screen.contains("Rename") && !screen.contains("Close"),
        "{screen}"
    );
    // The only choice is Open; Down then Enter still opens, never closes.
    w.keys(&["Down", "Escape", "Escape"]);
    w.wait_for_screen(|s| s.contains("Find a session"));
    // A plain session next to it can still be closed.
    w.type_text("plain");
    w.keys(&["Right", "Down", "Down", "Enter"]);
    w.wait_for_screen(|s| s.contains("Close plain?"));
    w.keys(&["Left", "Enter"]);
    wait_for(|| (!w.session_names().contains(&"plain".to_string())).then_some(()));
    assert_eq!(w.session_names(), ["claude-work", "driver"]);
}

#[test]
fn the_cursor_stays_on_new_through_refreshes() {
    let w = World::new();
    let work = w.dir("work");
    w.ta(&["-d", "api"], &work);
    w.start_picker(&work);
    w.keys(&["Up"]);
    w.wait_for_screen(|s| s.contains("▌ + New session here"));
    // The picker refreshes every 2 seconds; wait through two of them.
    std::thread::sleep(std::time::Duration::from_millis(4500));
    let screen = w.screen();
    assert!(screen.contains("▌ + New session here"), "{screen}");
}

#[test]
fn picker_strips_host_prefix_shows_managed_diamond_and_live_preview() {
    let w = World::new();
    let work = w.dir("work");
    // Session with host prefix whose pane prints a recognisable line.
    w.tmux(&[
        "new-session",
        "-d",
        "-s",
        "box_demo",
        "-c",
        work.to_str().unwrap(),
        "sh",
        "-c",
        "printf 'live-preview-marker-42\\n'; sleep 60",
    ]);
    // Second session managed by another tool, also with host prefix.
    w.tmux(&[
        "new-session",
        "-d",
        "-s",
        "box_managed",
        "-c",
        work.to_str().unwrap(),
        "sleep",
        "60",
    ]);
    w.tmux(&["set-option", "-t", "box_managed", "@managed-by", "cc-moss"]);

    w.start_picker_with_env(&work, &[("TA_HOST_PREFIX", "box")]);
    let screen = w.wait_for_screen(|s| s.contains("demo") && s.contains("managed"));
    assert!(!screen.contains("box_demo"), "{screen}");
    assert!(!screen.contains("box_managed"), "{screen}");
    assert!(screen.contains("◆"), "{screen}");

    // Filter to `demo` so it is selected and its live preview loads in the card.
    w.type_text("demo");
    let screen = w.wait_for_screen(|s| s.contains("live-preview-marker-42"));
    assert!(screen.contains("live-preview-marker-42"), "{screen}");
}
