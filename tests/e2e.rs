mod common;

use common::{stderr, stdout, World};

#[test]
fn direct_creates_then_reuses() {
    let w = World::new();
    let work = w.dir("work");
    let first = w.ta(&["-d", "foo"], &work);
    assert!(first.status.success(), "{}", stderr(&first));
    assert_eq!(stdout(&first), "foo\n");
    let again = w.ta(&["-d", "foo"], &work);
    assert!(again.status.success(), "{}", stderr(&again));
    assert_eq!(stdout(&again), "foo\n");
    assert_eq!(w.session_names(), ["foo"]);
}

#[test]
fn direct_cleans_a_name_tmux_cannot_take() {
    let w = World::new();
    let work = w.dir("work");
    assert_eq!(stdout(&w.ta(&["-d", "my notes"], &work)), "my-notes\n");
    assert_eq!(stdout(&w.ta(&["-d", "my notes"], &work)), "my-notes\n");
    assert_eq!(w.session_names(), ["my-notes"]);
}

#[test]
fn new_twice_in_same_dir_gets_dash_2() {
    let w = World::new();
    let shop = w.dir("webshop");
    assert_eq!(stdout(&w.ta(&["new", "-d"], &shop)), "webshop\n");
    assert_eq!(stdout(&w.ta(&["new", "-d"], &shop)), "webshop-2\n");
    assert_eq!(stdout(&w.ta(&["-d", "new", "api"], &shop)), "api\n");
    assert_eq!(w.session_names(), ["api", "webshop", "webshop-2"]);
}

#[test]
fn ls_lists_sessions() {
    let w = World::new();
    let work = w.dir("work");
    let empty = w.ta(&["ls"], &work);
    assert!(empty.status.success());
    assert!(
        stdout(&empty).contains("No sessions yet"),
        "{}",
        stdout(&empty)
    );
    w.ta(&["-d", "foo"], &work);
    let listed = stdout(&w.ta(&["ls"], &work));
    assert!(listed.contains("foo"), "{listed}");
    assert!(listed.contains("1 win"), "{listed}");
    assert!(listed.contains("~/work"), "{listed}");
}

#[test]
fn a_session_named_like_a_command_is_reachable() {
    let w = World::new();
    let work = w.dir("work");
    assert_eq!(stdout(&w.ta(&["-d", "--", "ls"], &work)), "ls\n");
    assert_eq!(w.session_names(), ["ls"]);
}

#[test]
fn missing_tmux_says_how_to_install() {
    let w = World::new();
    let work = w.dir("work");
    let empty = w.dir("empty-path");
    let out = w
        .command(&["ls"], &work)
        .env("PATH", &empty)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("tmux is not installed"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn a_name_and_a_command_together_are_refused() {
    let w = World::new();
    let work = w.dir("work");
    let out = w.ta(&["foo", "ls"], &work);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("not both"), "{}", stderr(&out));
    assert!(w.session_names().is_empty());
}

fn write_config(w: &World, text: &str) {
    let dir = w.home().join(".config/ta");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("config.toml"), text).unwrap();
}

#[test]
fn config_socket_is_used() {
    let w = World::new();
    let work = w.dir("work");
    write_config(&w, &format!("socket = \"{}\"\n", w.socket));
    let out = w
        .command(&["-d", "foo"], &work)
        .env_remove("TA_TMUX_SOCKET")
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(w.session_names(), ["foo"]);
}

#[test]
fn host_prefix_names_new_sessions() {
    let w = World::new();
    let work = w.dir("work");
    write_config(&w, "host_prefix = true\n");
    let new = w
        .command(&["new", "-d"], &work)
        .env("TA_HOST_PREFIX", "box")
        .output()
        .unwrap();
    assert_eq!(stdout(&new), "box_work\n", "{}", stderr(&new));
    let again = w
        .command(&["-d", "work"], &work)
        .env("TA_HOST_PREFIX", "box")
        .output()
        .unwrap();
    assert_eq!(stdout(&again), "box_work\n");
    assert_eq!(w.session_names(), ["box_work"]);
}

#[test]
fn broken_config_stops_before_tmux() {
    let w = World::new();
    let work = w.dir("work");
    write_config(&w, "sockett = \"x\"\n");
    let out = w.ta(&["-d", "foo"], &work);
    assert_eq!(out.status.code(), Some(1));
    let err = stderr(&out);
    assert!(err.contains(".config/ta/config.toml"), "{err}");
    assert!(err.contains("sockett"), "{err}");
    assert!(w.session_names().is_empty());
}

#[test]
fn config_language_is_used() {
    let w = World::new();
    let work = w.dir("work");
    write_config(&w, "language = \"zh\"\n");
    let out = w
        .command(&["ls"], &work)
        .env_remove("TA_LANG")
        .output()
        .unwrap();
    assert!(stdout(&out).contains("还没有会话"), "{}", stdout(&out));
}
