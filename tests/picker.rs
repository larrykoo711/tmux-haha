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
