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

#[test]
fn ls_json_has_every_field() {
    let w = World::new();
    let work = w.dir("work");
    w.ta(&["-d", "api"], &work);
    w.tmux(&["set-option", "-t", "api", "@managed-by", "cc-moss"]);
    w.ta(&["-d", "plain"], &work);
    let out = w.ta(&["ls", "--json"], &work);
    assert!(out.status.success(), "{}", stderr(&out));
    let v: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(v["schema"], 1);
    let sessions = v["sessions"].as_array().unwrap();
    let api = sessions.iter().find(|s| s["name"] == "api").unwrap();
    assert!(api["id"].as_str().unwrap().starts_with('$'), "{api}");
    assert_eq!(api["windows"], 1);
    assert_eq!(api["attached"], false);
    assert!(api["activity"].as_i64().unwrap() > 1_600_000_000, "{api}");
    assert!(api["path"].as_str().unwrap().ends_with("/work"), "{api}");
    assert_eq!(api["managed_by"], "cc-moss");
    let plain = sessions.iter().find(|s| s["name"] == "plain").unwrap();
    assert!(plain["managed_by"].is_null(), "{plain}");
}

#[test]
fn new_in_another_folder() {
    let w = World::new();
    let work = w.dir("work");
    let shop = w.dir("shop");
    let out = w.ta(&["new", "-d", "--dir", shop.to_str().unwrap()], &work);
    assert_eq!(stdout(&out), "shop\n", "{}", stderr(&out));
    let path = w.tmux(&["display-message", "-p", "-t", "shop", "#{session_path}"]);
    assert_eq!(path.trim(), shop.to_string_lossy());
    let missing = w.ta(&["new", "-d", "--dir", "/no/such/folder"], &work);
    assert_eq!(missing.status.code(), Some(1));
    assert!(
        stderr(&missing).contains("is not a folder"),
        "{}",
        stderr(&missing)
    );
    assert_eq!(w.session_names(), ["shop"]);
}

#[test]
fn close_with_yes_closes_only_that_one() {
    let w = World::new();
    let work = w.dir("work");
    w.ta(&["-d", "api"], &work);
    w.ta(&["-d", "api-v2"], &work);
    let out = w.ta(&["close", "api", "--yes"], &work);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "Closed api.\n");
    assert_eq!(w.session_names(), ["api-v2"]);
    let gone = w.ta(&["close", "api", "--yes"], &work);
    assert_eq!(gone.status.code(), Some(1));
    assert!(
        stderr(&gone).contains("there is no session called api"),
        "{}",
        stderr(&gone)
    );
}

#[test]
fn close_without_yes_off_a_terminal_refuses() {
    let w = World::new();
    let work = w.dir("work");
    w.ta(&["-d", "api"], &work);
    let out = w
        .command(&["close", "api"], &work)
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("add --yes"), "{}", stderr(&out));
    assert_eq!(w.session_names(), ["api"]);
}

#[test]
fn close_refuses_a_managed_session() {
    let w = World::new();
    let work = w.dir("work");
    w.ta(&["-d", "claude"], &work);
    w.tmux(&["set-option", "-t", "claude", "@managed-by", "cc-moss"]);
    let out = w.ta(&["close", "claude", "--yes"], &work);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("cc-moss manages this session"),
        "{}",
        stderr(&out)
    );
    assert_eq!(w.session_names(), ["claude"]);
}

#[test]
fn rename_cleans_and_refuses_managed() {
    let w = World::new();
    let work = w.dir("work");
    w.ta(&["-d", "api"], &work);
    w.ta(&["-d", "taken"], &work);
    let out = w.ta(&["rename", "api", "taken"], &work);
    assert_eq!(stdout(&out), "taken-2\n", "{}", stderr(&out));
    let out = w.ta(&["rename", "taken-2", "my notes"], &work);
    assert_eq!(stdout(&out), "my-notes\n");
    assert_eq!(w.session_names(), ["my-notes", "taken"]);
    w.tmux(&["set-option", "-t", "taken", "@managed-by", "cc-moss"]);
    let out = w.ta(&["rename", "taken", "other"], &work);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("cc-moss manages this session"),
        "{}",
        stderr(&out)
    );
    assert_eq!(w.session_names(), ["my-notes", "taken"]);
}

#[test]
fn doctor_checks_everything() {
    let w = World::new();
    let work = w.dir("work");
    w.config("[folders]\nroots = [\"~/work\"]\n");
    w.ta(&["-d", "api"], &work);
    let out = w.ta(&["doctor"], &work);
    let text = stdout(&out);
    assert_eq!(out.status.code(), Some(0), "{text}");
    assert!(text.contains("✓ tmux") && text.contains("tmux 3"), "{text}");
    assert!(text.contains("1 session(s)"), "{text}");
    assert!(
        text.contains("✓ config") && text.contains("~/.config/ta/config.toml"),
        "{text}"
    );
    assert!(
        text.contains("✓ folder root") && text.contains("~/work"),
        "{text}"
    );
    assert!(text.contains("ta is ready."), "{text}");
}

#[test]
fn doctor_reports_a_broken_config() {
    let w = World::new();
    let work = w.dir("work");
    w.config("sockett = \"x\"\n");
    let out = w.ta(&["doctor"], &work);
    let text = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{text}");
    assert!(
        text.contains("✗ config") && text.contains("sockett"),
        "{text}"
    );
    // The other checks still ran.
    assert!(text.contains("✓ tmux"), "{text}");
    assert!(text.contains("1 thing(s) to fix above."), "{text}");
    // config shows the mistake too, instead of failing to start.
    let out = w.ta(&["config"], &work);
    assert_eq!(out.status.code(), Some(1));
    assert!(stdout(&out).contains("sockett"), "{}", stdout(&out));
}

#[test]
fn doctor_without_tmux() {
    let w = World::new();
    let work = w.dir("work");
    let empty = w.dir("empty-path");
    let out = w
        .command(&["doctor"], &work)
        .env("PATH", &empty)
        .output()
        .unwrap();
    let text = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{text}");
    assert!(
        text.contains("✗ tmux") && text.contains("not installed"),
        "{text}"
    );
    assert!(text.contains("! zoxide"), "{text}");
}

#[test]
fn config_says_where_each_value_comes_from() {
    let w = World::new();
    let work = w.dir("work");
    w.config("host_prefix = true\nlanguage = \"en\"\n");
    let text = stdout(&w.ta(&["config"], &work));
    let line = |key: &str| {
        text.lines()
            .find(|l| l.trim_start().starts_with(key))
            .unwrap_or_else(|| panic!("{key}: {text}"))
            .to_string()
    };
    assert!(line("socket").ends_with("TA_TMUX_SOCKET"), "{text}");
    assert!(
        line("host_prefix").contains("true") && line("host_prefix").ends_with("config file"),
        "{text}"
    );
    assert!(line("language").ends_with("TA_LANG"), "{text}");
    assert!(line("folders.roots").ends_with("default"), "{text}");
    let from_flag = stdout(&w.ta(&["config", "--socket", "other"], &work));
    assert!(
        from_flag.contains("other") && from_flag.contains("--socket"),
        "{from_flag}"
    );
    let path = stdout(&w.ta(&["config", "--path"], &work));
    assert!(path.trim().ends_with(".config/ta/config.toml"), "{path}");
    assert_eq!(path.lines().count(), 1);
}
