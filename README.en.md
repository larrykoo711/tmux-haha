# tmux-haha

[中文](README.md)

`ta`: one small command to find, start and switch tmux sessions.

```sh
ta api-review    # go to that session, or start it in this folder and go there
ta new           # start a session in this folder, named after it (-2, -3… if taken)
ta ls            # list sessions (* means a terminal is attached)
```

- Inside tmux it switches to the session; outside tmux it attaches.
- `-d` starts or finds the session and prints its name without going there, for scripts.
- `--socket <name|path>` (or `TA_TMUX_SOCKET`) picks the tmux server; by default tmux's own.
- Names can be in any script, emoji included; what tmux can't take (spaces, `.`, `:`…) becomes `-`.

## Living with other tools

`ta` only does what you ask: it doesn't change global tmux options, bind keys or end the tmux server. Sessions other tools manage on the same server are left alone.

## Install

From source: `cargo install --git https://github.com/larrykoo711/tmux-haha` (needs Rust 1.89+ and tmux).

## License

MIT. Parts are adapted from [cc-moss](https://github.com/larrykoo711/cc-moss) (MIT).
