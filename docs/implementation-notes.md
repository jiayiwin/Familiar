# 实现说明

这份文档记录睿洱的实现细节和测试方法。面向用户的说明见 [README.md](../README.md) / [README.zh-CN.md](../README.zh-CN.md)。

## hook 映射

- **pid**：hook 经 shell 启动，emitter 的直接父进程马上就会退出。所以 emitter 沿进程树往上，找第一个 `claude`、`codex`、`node` 或 `bun` 进程作为 agent pid。
- **Claude Code**：
  - 接了 `PostToolUseFailure`（工具失败）和 `PermissionRequest`（等待批准）。
  - `Notification` 只有 `permission_prompt` 和 `elicitation_dialog` 算 attention。`idle_prompt` 在回合结束约 60 秒后触发，不算 attention，否则每轮结束都会变成「等你」。
- **Codex**：按官方文档核对，并实测了 codex-cli 0.162.1。映射表在 `adapters/codex/mapping.yaml`。
  - Codex 在 Windows 上读文件、搜索也走 `Bash` 工具（PowerShell）。emitter 只看命令的第一个词（如 `Get-Content`、`rg`、`git status`）来判断是不是读取类操作，判断结果作为 `detail.read` 发出，命令原文不离开 emitter。
  - `apply_patch` 的文件名从补丁头里取 basename。
  - `PostToolUse` 没有成功或失败标记，按成功处理。
  - `Interrupt` 映射为 aborted。
  - Codex 没有「等待输入」的 hook，只有 `PermissionRequest` 算 attention。

## 几个决定

- **每个 agent 一只宠物**：同一个 agent 的所有会话聚合成一只的状态，等你的会话优先。会话多于一个时角上显示数量。
- **僵尸检测**：每 5 秒检查一次 agent 进程，关掉终端后那个会话几秒内就会消失。
- **宠物移动**：直接以 60 fps 移动宠物自己的小窗口。覆盖层只画虫子。
- **「催你」**：这条规则本身就是提醒的一部分，是唯一允许在有会话等你时触发的互动（规则里标了 `during_attention: true`）。其余互动在等待期间全部冻结。
- **糖果的出法**（`world.rs` 的 `catch`，目录在 `assets/treats.yaml`）：短于 5 秒的回合不给，否则每条简单回复都会得一颗。先按回合时长掷稀有度，再从「此刻对得上」的种类里挑：`from`（agent / kegel / break）、`hours`、`months` 都对得上才进候选，带条件的权重是日常款的三倍；你自己挣的固定是话梅和薄荷叶。代码和数据库里还叫 `fish`（表 `catches` 的 outcome、`fish_id`），前十二种沿用了原来十二条鱼的 id，所以以前攒的还在。
- **它自己会做的事**（`crates/petd/src/think.rs`）：每 20 秒看一次「距离上次键鼠输入多久」和各 agent 的状态，认出久坐、深夜、连续卡住、长活做完、离开后回来这几种情况，每种只回应一个动作。两次之间至少隔 10 分钟；专注模式和「安静」时不动。每次动作都往 `events` 表记一条 `noticed`，小屋和图鉴的日记读的就是它。同一个循环累计「你在桌前」的时间和起身次数（kv 里的 `desk:<日期>`）。
- **两只的联动**（同在 `think.rs`）：`Plan` 里除了主角的动作，还有「谁走过去」（`walk`）和「另一只做什么」（`with`）。两只一起待过的天数记在 kv 的 `duo:days` / `duo:last`，不到 2 天只旁听，之后合奏、共舞。每只宠物在自己的窗口里出声，靠 `pet.act` 消息里的同一个墙上时刻 `at` 对拍（走过去的留 2.8 秒，原地的留 0.9 秒）；走过去的那只曲终回原位，后一场表演会作废前一场的回家定时器。菜单里手动点的表演走同一条路（`think::asked` → `think::stage`）。台词按宠物分，在 `voice.rs`（守护进程主动说的）和 `app/src/voice.ts`（窗口里的状态句）。
- **发布版才有的坑：内容安全策略**。Tone.js 用 blob 生成的 Worker 给节拍计时；`tauri.conf.json` 的 CSP 不放行 `worker-src blob:` 时，Worker 悄悄失败（不抛异常），画面照动、音符一个不响，而开发模式没有 CSP，看不出来。`app/src-tauri/tests/csp.rs` 守着这一条。
- **所有页面开在一个窗口里**（`app/shell.html`、`app/src/shell.ts`，窗口标签 `home`）：无系统边框、透明，标签栏和阴影自己画；每个页面是一个 iframe 标签，后台标签不销毁（电台接着放）。页面是窗口的子框架，没有自己的宿主通道，`public/embed.js` 在页面脚本之前把父窗口的 `__TAURI_INTERNALS__` 借给它；宿主发给页面的事件由外壳用 `postMessage` 转交（`api.ts` 的 `onHost`），页面要关自己就发 `close-tab`（`closeSelf`）。宿主侧只有一个入口 `windows::open_tab`。
- **糖是挣来的**：agent 做完一轮得一颗之外，`ops::earn_fish` 给两种自己挣的：跟完一组提肛（宠物窗口在整组做完时 `POST /v1/earn`，守护进程核对确实开始过一组、时长对得上）和久坐后起身（`think.rs` 记下离开前坐了多久，回来时满 30 分钟就给）。每天上限 6 颗，记在 kv 的 `earned:<日期>`。
- **没有 agent 时只留一只**：`hub.rs` 的 `groups()` 永远保留 `claude-code` 这一只，其余 agent 的宠物在它最后一个会话离线后消失。拖动主宠时，其余宠物由 `physics::drag` 带着保持相对位置一起走，松手后各自按同样的速度落地。
- **宠物追着拍虫子**（`overlay.ts` 的 `hunt`）：每块屏幕的 overlay 每 0.25 秒看一次，落地超过 1.1 秒的小虫由它自己的宠物去追（`pet_chase` 命令让宿主沿地面跑到某个 x，`physics::chase`），够得着就发 `swat` 给宠物窗口做扑的动作，再调 `/v1/world/squash`。Boss 把自己的宠物压住（状态 stone），同屏其他空闲的宠物都来帮，每 0.9 秒拍一下。被压住的宠物和帮忙的宠物不参与防重叠（`spawn_separator`），否则会被弹开。只有用户亲手甩出去的才会挂到屏幕边上，防重叠和生气跳开的那种弹开不会。
- **挂在屏幕边上**（`physics.rs` 的 `fly`）：飞行中以够快的速度撞上所有显示器合起来的左右边或顶边，就停在那儿（每次飞行一次），发 `phys` 事件 `cling`；之后计时、或收到 `let_go`（喂糖、被点、agent 在等你）时发 `slip`，继续按重力落地，再滑步回到起飞前的位置。中间隔着另一块显示器的边不算边。
- **打开的页面跟着换肤**：`skin.ts` 的 `followSkin` 订阅 `pet.state`，页面所属的那个 agent 换了宠物就重载（电台只换座位，不停播）。
- **每只宠物是一整套皮肤**（`crates/petd/src/cast.rs`、`app/src/skin.ts`）：`PetView.skin` 说这个 agent 由谁陪；窗口按它取颜色、图层、乐器和口吻。新增一只：切层放进 `design/pet-art/<id>/rig`，在 `sync_app.py` 的 `PETS` 里加上，`cast.rs` 里把 `here` 改成 true，采样放进 `app/public/audio/<乐器>/`（带 LICENSE.txt），`perform.js` 里加它的谱子。
- **身体是拆层绑定**（`app/src/rig/`）：一张原画切成身体、耳朵、眼、嘴、配件和背面，由 `rig.js` 驱动姿态、表情和舞步，`perform.js` 负责五种乐器（Tone.js 采样器）和跳舞的节拍。身体分两类：紫梦的长耳兔（`kind: lop`），和其余四只共用的直立身体（头绕脖子转，图层按 `rig.json` 的 `order` 从后往前画；头顶的卷须、菌盖是带弹簧的部件）。乐器是引擎画出来的矢量图（和紫梦的琴键、麻薯的大提琴一样）：泡泡的长笛跟着头走，音从笛尾变成泡泡；菇菇的木琴浮在身前，两根琴槌各管一半琴条，菌盖上的斑点（`spots`）跟着亮；茸茸的竖琴立在身侧。`pet.play` 是给外部驱动用的姿态入口（站位、离地、倾斜、转身、看向哪里、表情），小屋里的互动（`app/src/play.ts`）和挂在屏幕边上的姿态都走它。背面没有尾巴的两只（菇菇、泡泡）由 `design/pet-art/tools/back_tail.py` 按身体的颜色补一个小尾巴。切层脚本和预览页在 `design/pet-art/`，改完运行 `python design/pet-art/tools/sync_app.py` 再跑 `node design/pet-art/preview/rig.test.mjs`。
- **位置**：保存在 `windows.json`，没放进 `config.yaml`，避免拖一下宠物就改写你的配置。
- **旧数据**：早先版本的配置项（路由模式、宠物包等）加载时忽略；旧数据库里多出来的表不再读写，也不删除。
- **Rust 版本**：工作区设了 `rust-version = 1.88` 并启用 resolver 3，Cargo 会自动选兼容的 Tauri 版本（目前是 2.11）。

## 已知限制

- **共享屏幕检测**：靠会议软件「正在共享」工具条的窗口标题 / 类名识别，内置名单覆盖常见软件，不保证每个版本都认得出；认不出时在 `config.yaml` 里加 `meeting: { share_windows: [...] }`。
- **只有紫梦**：另外四只宠物只有原画，还没有切层。两个 agent 的宠物现在都是紫梦。
- **工程**：还不支持 macOS；安装包没有代码签名，第一次运行时 Windows SmartScreen 可能会拦一下，点「更多信息 → 仍要运行」。
- **双击跳转**：从 agent 进程沿进程树往上找第一个有窗口的进程；Windows Terminal 只能切到窗口，切不到具体标签页。

## 测试

```bash
cargo test --workspace                 # 全部 Rust 测试，含一个把守护进程真跑起来的端到端测试（crates/petd/tests/api.rs）
cargo test -p rillbuddy-emit -p rillbuddy-petd   # 只跑 emitter 和守护进程核心
cd app && npx tsc --noEmit -p .        # 前端类型检查
node design/pet-art/preview/rig.test.mjs       # 绑定的姿态测试
```

手动喂事件（不需要 agent）：

```bash
rillbuddy-emit --source claude-code < payload.json
RILLBUDDY_DEBUG=1 rillbuddy-emit --source codex < payload.json   # 把处理过程打到 stderr
```

在浏览器里调试页面：先 `cd app && npx vite`，然后打开 `http://localhost:1420/index.html?pet=claude-code&port=47321&token=<~/.rillbuddy/token 的内容>`。`gallery.html`、`report.html` 的用法相同；`room.html?demo=1`、`menu.html`、`tray.html`、`letter.html` 不连后台也能看（用示例数据）；`index.html?act=dance|piano|kegel|speak&text=…` 直接看一段表演。

调试用的 `/pet` 命令：`/pet fling [宠物] [vx] [vy]` 甩出宠物测试物理；`/pet demo feed|card [宠物]` 重放喂食和重置卡的特效。

## 架构要点

- **`rillbuddy-emit`**：hook 发射器。总耗时 ≤ 300 ms，退出码恒为 0，不写 stdout。petd 不在时先把事件写入 spool，再以脱离进程组的方式拉起 petd。
- **`petd`**：会话状态机与聚合优先级；spool 补读、单实例锁；本地 HTTP + WebSocket（接口列表见 `crates/petd/src/server.rs` 顶部注释，每个请求都要带 `X-RillBuddy-Token` 头或 `?token=`）。
- **世界层**：每 500 ms 评估一次互动规则（`crates/petd/assets/interactions.yaml`）。
- **数据保留**：每轮的明细保留 90 天，更早的汇总成每日数据。
