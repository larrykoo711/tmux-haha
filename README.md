# tmux-haha

[English](README.en.md)

`ta`：一个小命令，找到、新建、切换 tmux 会话。

```sh
ta               # 打开选择器：打字搜索，↑↓ 移动，↵ 进入，→ 改名或关闭，tab 切到窗口页、目录页，esc 退出
ta 接口评审      # 有这个会话就进去，没有就在当前目录新建再进去
ta new           # 在当前目录新建一个会话，以目录命名，重名自动加 -2
ta ls            # 列出会话（* 表示有终端正接着）
```

- 选择器里搜不到时按 ↵，就以搜索词新建会话。关闭会话要先确认（默认「再想想」），你所在的会话不能在这里关。
- 目录页列出 zoxide 记得的常用目录，以及配置里 `[folders] roots` 下的子目录；选中按 ↵，目录里已有会话就进去，没有就新建一个。
- 在 tmux 里运行会直接切过去，在 tmux 外会接上去。
- `-d` 只新建或找到会话、打印它的名字，不进去，方便写脚本。
- `--socket <名字|路径>`（或环境变量 `TA_TMUX_SOCKET`）指定 tmux server，默认用 tmux 自己的默认 server。
- 会话名可以用中文和 emoji；tmux 不接受的字符（空格、`.`、`:` 等）会变成 `-`。

## 配置

可选，放在 `~/.config/ta/config.toml`（设了 `XDG_CONFIG_HOME` 就放在它下面）。没有这个文件时用默认值；写错了 `ta` 会说出是哪个文件、哪一行，不会悄悄忽略。

```toml
socket = "work"        # 用哪个 tmux server（名字或路径）；--socket 和 TA_TMUX_SOCKET 优先
host_prefix = false    # true：新会话名前加本机名，如 mbp_api（本机名可用 TA_HOST_PREFIX 指定）
language = "auto"      # auto、zh 或 en；TA_LANG 优先

[folders]
roots = ["~/code"]     # 目录页从这些文件夹里列出子文件夹
```

## 和别的工具共处

`ta` 只做你让它做的事：不改全局 tmux 设置、不绑按键、不结束 tmux server。关闭会话前一定先问你。

别的工具可以在自己管理的会话上设一个 tmux 会话选项，告诉 `ta` 别动它：

```sh
tmux set-option -t <会话> @managed-by <工具名>
```

带这个选项的会话，`ta` 照常列出并标出工具名，但只能进入，不能改名、不能关闭。[cc-moss](https://github.com/larrykoo711/cc-moss) 会给它的 Claude Code 会话设上 `@managed-by cc-moss`。

## 安装

Homebrew（macOS、Linux）：

```sh
brew install larrykoo711/tap/tmux-haha
```

也可以从 [Releases](https://github.com/larrykoo711/tmux-haha/releases) 下载对应平台的包，把里面的 `ta` 放进 PATH；或者从源码装：`cargo install --git https://github.com/larrykoo711/tmux-haha`（需要 Rust 1.89+）。都需要先装好 tmux。

## 许可

MIT。部分代码改编自 [cc-moss](https://github.com/larrykoo711/cc-moss)（MIT）。
