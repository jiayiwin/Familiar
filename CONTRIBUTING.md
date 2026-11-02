# Contributing

[English](#english) · [中文](#中文)

## English

Thanks for helping with Familiar. Bug reports, pet packs, translations and code are all welcome.

### Build

Requirements: Rust 1.88+, Node 18+, WebView2 (included with Windows 10/11).

```bash
cd app && npm install && cd ..
node tools/build.mjs --debug     # fast build; drop --debug for release, add --bundle for the installer
```

See [Build from source](README.md#build-from-source) for running a source build against Claude Code and Codex.

### Before you open a pull request

```bash
cargo test --workspace
cargo clippy --workspace --all-targets   # must be warning-free
cd app && npx tsc --noEmit -p .
```

Feed the daemon events without an agent:

```bash
familiar-emit --source claude-code < payload.json
FAMILIAR_DEBUG=1 familiar-emit --source codex < payload.json   # trace to stderr
```

Debug a page in a browser: run `cd app && npx vite`, then open `http://localhost:1420/index.html?pet=claude-code&port=47321&token=<contents of ~/.familiar/token>` (same for `gallery.html` and `welcome.html`). More notes are in [docs/implementation-notes.md](docs/implementation-notes.md).

### Every user-facing string needs both languages

The UI is Chinese and English, switched by `language:` in `config.yaml`.

- **Rust**: `petd::i18n::t("中文", "English")` for plain strings, `tr!("中文 {x}", "English {x}")` for formatted ones.
- **TypeScript**: `t("中文", "English")` from `app/src/i18n.ts`; in static HTML, `<span data-en="English">中文</span>` (also `data-en-title`, `data-en-placeholder`).
- **Data files and packs**: add the `*_en` field next to the Chinese one — `name_en`, `note_en` in `fish.yaml`, `bubble_en` in `interactions.yaml`, `lines_en` in `balance.yaml`, `name_en`, `description_en`, `bubbles_en`, `tags_en` in `pet.json`.

### Other rules

- The emitter runs on every hook call: keep it fast (≤ 300 ms), always exit 0, never write to stdout.
- Never forward prompts, commands, file contents or tool output from the emitter to the daemon.
- Nothing may make network requests; the daemon only listens on `127.0.0.1`.
- Pet packs you contribute must be your own work or properly licensed.

### Commit style

Short, imperative, sentence-case subject lines that say what changed for the user, e.g. `Clearer rock-paper-scissors and duel feedback` or `Fix blank gallery from the ring menu`. Add a body when the why is not obvious. One logical change per commit.

## 中文

感谢参与 Familiar。欢迎提 bug、贡献宠物包、翻译和代码。

### 构建

需要 Rust 1.88+、Node 18+、WebView2（Windows 10/11 自带）。

```bash
cd app && npm install && cd ..
node tools/build.mjs --debug     # 快速构建；去掉 --debug 是 release，加 --bundle 出安装包
```

如何让源码构建接入 Claude Code 和 Codex，见 [从源码构建](README.zh-CN.md#从源码构建)。

### 提 PR 之前

```bash
cargo test --workspace
cargo clippy --workspace --all-targets   # 不能有警告
cd app && npx tsc --noEmit -p .
```

不需要 agent 也能手动喂事件：

```bash
familiar-emit --source claude-code < payload.json
FAMILIAR_DEBUG=1 familiar-emit --source codex < payload.json   # 把处理过程打到 stderr
```

在浏览器里调试页面：先 `cd app && npx vite`，然后打开 `http://localhost:1420/index.html?pet=claude-code&port=47321&token=<~/.familiar/token 的内容>`（`gallery.html`、`welcome.html` 同理）。更多说明见 [docs/implementation-notes.md](docs/implementation-notes.md)。

### 所有面向用户的文字都要有中英两种

界面有中文和英文，由 `config.yaml` 的 `language:` 切换。

- **Rust**：普通字符串用 `petd::i18n::t("中文", "English")`，带格式的用 `tr!("中文 {x}", "English {x}")`。
- **TypeScript**：用 `app/src/i18n.ts` 里的 `t("中文", "English")`；静态 HTML 写成 `<span data-en="English">中文</span>`（还有 `data-en-title`、`data-en-placeholder`）。
- **数据文件和宠物包**：在中文字段旁边加 `*_en` 字段——`fish.yaml` 的 `name_en`、`note_en`，`interactions.yaml` 的 `bubble_en`，`balance.yaml` 的 `lines_en`，`pet.json` 的 `name_en`、`description_en`、`bubbles_en`、`tags_en`。

### 其他约定

- emitter 每次 hook 都会运行：保持快速（≤ 300 ms），退出码恒为 0，不写 stdout。
- emitter 绝不把提示词、命令、文件内容或工具输出转发给守护进程。
- 不允许任何联网请求；守护进程只监听 `127.0.0.1`。
- 贡献的宠物包必须是你自己的作品，或者有正当授权。

### 提交信息

用简短的英文祈使句作标题，首字母大写，说清楚对用户来说改了什么，比如 `Clearer rock-paper-scissors and duel feedback`、`Fix blank gallery from the ring menu`。原因不明显时写正文。一个提交只做一件事。
