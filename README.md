# tmux-haha

[English](README.en.md)

`ta`：一个小命令，找到、新建、切换 tmux 会话。

```sh
ta 接口评审      # 有这个会话就进去，没有就在当前目录新建再进去
ta new           # 在当前目录新建一个会话，以目录命名，重名自动加 -2
ta ls            # 列出会话（* 表示有终端正接着）
```

- 在 tmux 里运行会直接切过去，在 tmux 外会接上去。
- `-d` 只新建或找到会话、打印它的名字，不进去，方便写脚本。
- `--socket <名字|路径>`（或环境变量 `TA_TMUX_SOCKET`）指定 tmux server，默认用 tmux 自己的默认 server。
- 会话名可以用中文和 emoji；tmux 不接受的字符（空格、`.`、`:` 等）会变成 `-`。

## 和别的工具共处

`ta` 只做你让它做的事：不改全局 tmux 设置、不绑按键、不结束 tmux server。同一个 tmux server 上别的工具管理的会话，它都不会碰。

## 安装

从源码：`cargo install --git https://github.com/larrykoo711/tmux-haha`（需要 Rust 1.89+ 和 tmux）。

## 许可

MIT。部分代码改编自 [cc-moss](https://github.com/larrykoo711/cc-moss)（MIT）。
