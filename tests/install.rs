//! `ta install`, `ta completions`, and doctor's completions line, in a
//! temporary HOME.

mod common;

use common::{stderr, stdout, World};

fn zsh(w: &World, args: &[&str]) -> std::process::Output {
    w.command(args, &w.home())
        .env("SHELL", "/bin/zsh")
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap()
}

#[test]
fn completions_for_each_shell() {
    let w = World::new();
    for (shell, mark) in [
        ("zsh", "#compdef ta"),
        ("bash", "_ta()"),
        ("fish", "complete -c ta"),
    ] {
        let out = stdout(&w.ta(&["completions", shell], &w.home()));
        assert!(out.contains(mark), "{shell}: {out}");
        assert!(out.contains("doctor"), "{shell} knows the commands");
    }
}

#[test]
fn install_adds_blocks_once() {
    let w = World::new();
    std::fs::write(w.home().join(".zshrc"), "export A=1\n").unwrap();
    for _ in 0..2 {
        let out = zsh(&w, &["install", "--yes"]);
        assert!(out.status.success(), "{}", stderr(&out));
    }
    let rc = std::fs::read_to_string(w.home().join(".zshrc")).unwrap();
    assert_eq!(rc.matches("# >>> ta: completions >>>").count(), 1, "{rc}");
    assert!(rc.contains("source <(ta completions zsh)"), "{rc}");
    let conf = std::fs::read_to_string(w.home().join(".tmux.conf")).unwrap();
    assert_eq!(conf.matches("display-popup -E").count(), 1, "{conf}");
    assert!(conf.contains("bind-key T"), "{conf}");
}

#[test]
fn uninstall_restores_the_file_byte_for_byte() {
    let w = World::new();
    let zshrc = "export A=1\n# my own block\nalias x=y\n";
    let tmux = "set -g mouse on\n";
    std::fs::write(w.home().join(".zshrc"), zshrc).unwrap();
    std::fs::write(w.home().join(".tmux.conf"), tmux).unwrap();
    zsh(&w, &["install", "--yes"]);
    let fish = w
        .command(
            &["install", "--yes", "--shell", "fish", "--no-tmux-key"],
            &w.home(),
        )
        .output()
        .unwrap();
    assert!(fish.status.success(), "{}", stderr(&fish));
    let fish_file = w.home().join(".config/fish/completions/ta.fish");
    assert!(fish_file.exists());
    let out = zsh(&w, &["install", "--uninstall", "--yes"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        std::fs::read_to_string(w.home().join(".zshrc")).unwrap(),
        zshrc
    );
    assert_eq!(
        std::fs::read_to_string(w.home().join(".tmux.conf")).unwrap(),
        tmux
    );
    assert!(!fish_file.exists());
}

#[test]
fn the_tmux_key_is_added_only_when_asked() {
    let w = World::new();
    std::fs::write(w.home().join(".zshrc"), "").unwrap();
    // Off a terminal, without --yes: nothing is written.
    let out = zsh(&w, &["install"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("add --yes"), "{}", stderr(&out));
    assert_eq!(
        std::fs::read_to_string(w.home().join(".zshrc")).unwrap(),
        ""
    );
    assert!(!w.home().join(".tmux.conf").exists());
    // --no-tmux-key: completions only.
    let out = zsh(&w, &["install", "--yes", "--no-tmux-key"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(!w.home().join(".tmux.conf").exists());
    assert!(std::fs::read_to_string(w.home().join(".zshrc"))
        .unwrap()
        .contains("ta completions zsh"));
}

#[test]
fn doctor_sees_the_completions() {
    let w = World::new();
    std::fs::write(w.home().join(".zshrc"), "").unwrap();
    let before = stdout(&zsh(&w, &["doctor"]));
    assert!(
        before.contains("! completions") && before.contains("ta install"),
        "{before}"
    );
    zsh(&w, &["install", "--yes", "--no-tmux-key"]);
    let after = stdout(&zsh(&w, &["doctor"]));
    assert!(
        after.contains("✓ completions") && after.contains("zsh"),
        "{after}"
    );
}
