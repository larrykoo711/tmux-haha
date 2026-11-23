# tmux-haha

[中文](README.md)

`ta`: one small command to find, start and switch tmux sessions.

```sh
ta               # open the picker: type to search, ↑↓ to move, ↵ to go, → to rename or close, tab for windows and folders, esc to quit
ta api-review    # go to that session, or start it in this folder and go there
ta new           # start a session in this folder, named after it (-2, -3… if taken)
ta ls            # list sessions (* means a terminal is attached)
```

- In the picker, ↵ on a search with no match starts a session with that name. Closing a session asks first (Keep is the default), and the session you are in can't be closed from there.
- The folders tab lists the folders zoxide knows and the ones inside `[folders] roots` from the config; ↵ goes to the session already in that folder, or starts one there.
- Inside tmux it switches to the session; outside tmux it attaches.
- `-d` starts or finds the session and prints its name without going there, for scripts.
- `--socket <name|path>` (or `TA_TMUX_SOCKET`) picks the tmux server; by default tmux's own.
- Names can be in any script, emoji included; what tmux can't take (spaces, `.`, `:`…) becomes `-`.

## Configuration

Optional, in `~/.config/ta/config.toml` (under `XDG_CONFIG_HOME` when that is set). Without the file, defaults apply; if it has a mistake, `ta` says which file and where, rather than ignoring it.

```toml
socket = "work"        # which tmux server (a name or a path); --socket and TA_TMUX_SOCKET win
host_prefix = false    # true: new session names start with this machine's name, e.g. mbp_api (set it with TA_HOST_PREFIX)
language = "auto"      # auto, zh or en; TA_LANG wins

[folders]
roots = ["~/code"]     # the folder tab lists the folders inside these
```

## Living with other tools

`ta` only does what you ask: it doesn't change global tmux options, bind keys or end the tmux server, and it always asks before closing a session.

Another tool can tell `ta` to keep its hands off a session by setting one tmux session option:

```sh
tmux set-option -t <session> @managed-by <tool>
```

`ta` still lists such a session, with the tool's name, but only goes to it: no renaming, no closing. [cc-moss](https://github.com/larrykoo711/cc-moss) sets `@managed-by cc-moss` on its Claude Code sessions.

## Install

From source: `cargo install --git https://github.com/larrykoo711/tmux-haha` (needs Rust 1.89+ and tmux).

## License

MIT. Parts are adapted from [cc-moss](https://github.com/larrykoo711/cc-moss) (MIT).
