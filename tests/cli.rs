//! Help, version and usage mistakes, in both languages.

mod common;

use common::{stderr, stdout, World};

#[test]
fn help_follows_the_language() {
    let w = World::new();
    let work = w.dir("work");
    let zh = stdout(
        &w.command(&["--help"], &work)
            .env("TA_LANG", "zh")
            .output()
            .unwrap(),
    );
    assert!(
        zh.contains("用法：") && zh.contains("命令：") && zh.contains("选项："),
        "{zh}"
    );
    assert!(zh.contains("在当前目录新建会话"), "{zh}");
    assert!(!zh.contains("Usage:") && !zh.contains("Options:"), "{zh}");
    let zh_sub = stdout(
        &w.command(&["help", "new"], &work)
            .env("TA_LANG", "zh")
            .output()
            .unwrap(),
    );
    assert!(zh_sub.contains("会话名（默认用目录名）"), "{zh_sub}");
    assert!(
        zh_sub.contains("只新建或找到会话"),
        "global options too: {zh_sub}"
    );
    let en = stdout(&w.ta(&["--help"], &work));
    assert!(
        en.contains("Usage:") && en.contains("Start a new session"),
        "{en}"
    );
}

#[test]
fn version_is_plain_when_piped() {
    let w = World::new();
    let work = w.dir("work");
    let want = format!("ta {}\n", env!("CARGO_PKG_VERSION"));
    for args in [&["version"][..], &["-V"], &["--version"]] {
        let out = w.ta(args, &work);
        assert!(out.status.success(), "{args:?}");
        assert_eq!(stdout(&out), want, "{args:?}");
    }
    // No tmux needed to print a version.
    let empty = w.dir("empty-path");
    let out = w
        .command(&["version"], &work)
        .env("PATH", &empty)
        .output()
        .unwrap();
    assert_eq!(stdout(&out), want);
}

#[test]
fn usage_errors_follow_the_language() {
    let w = World::new();
    let work = w.dir("work");
    let zh = w
        .command(&["--bogus"], &work)
        .env("TA_LANG", "zh")
        .output()
        .unwrap();
    assert_eq!(zh.status.code(), Some(2));
    let err = stderr(&zh);
    assert!(
        err.contains("不认识的参数 `--bogus`") && err.contains("用法："),
        "{err}"
    );
    let en = w.ta(&["--bogus"], &work);
    assert_eq!(en.status.code(), Some(2));
    assert!(
        stderr(&en).contains("unexpected argument"),
        "{}",
        stderr(&en)
    );
    assert!(w.session_names().is_empty());
}
