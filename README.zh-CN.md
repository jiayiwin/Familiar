# Familiar

[English](README.md)

Familiar 是一个纯本地运行的桌宠系统：把 Claude Code 和 Codex 的真实工作状态显示成桌面宠物。每个会话对应一只宠物——agent 思考时它在想，调工具时它在敲，需要你批准时它第一时间挥手提醒，完成时庆祝，出错时垂头丧气。干活的间隙还有一层轻量的玩法：钓鱼、拍虫子、Claude Code 宠物和 Codex 宠物之间的拔河和小游戏，再加上宠物图鉴、每日报告和自制宠物包。所有东西都在你本机运行：不用注册账号，没有服务器，不收集任何数据。

目前只支持 Windows 10/11。技术栈：Rust、Tauri 2、TypeScript/Canvas。

<!-- screenshot：桌面上的两只阵营宠物，其中一只在挥手提醒 -->

## 目录

- [功能](#功能)
- [快速开始](#快速开始)
- [使用](#使用)
- [玩法与互动](#玩法与互动)
- [配置](#配置)
- [自制宠物包](#自制宠物包)
- [从源码构建](#从源码构建)
- [目录结构](#目录结构)
- [隐私与安全](#隐私与安全)
- [常见问题](#常见问题)
- [路线图](#路线图)
- [许可证](#许可证)

## 功能

### 状态一眼可见

- **真实的会话状态**，直接来自 agent 自己的 hook：思考、读文件、调工具、等你、完成、出错、被打断、睡着。
- **每个阵营一只主宠物，每个会话一个小分身**（默认的 `per-camp` 模式）。等你处理的分身排在最靠近主宠物的位置，头上冒感叹号，脚下显示项目名。其他路由模式：`single`、`per-rule`、`per-session`。
- **阵营标签**：阵营宠物脚下有橙色"Claude"、青色"Codex"小标签。两个阵营用同一个包时，Codex 那只会色相旋转 180°。
- **悬停 1 秒**显示会话卡片：项目、agent、状态、已运行时长和最近的工具。
- **双击宠物**把对应会话的终端或 IDE 调到前台（双击主宠物跳到最需要你处理的会话）。
- **僵尸会话检测**：每 5 秒检查一次 agent 进程，关掉终端后分身几秒内就会消失。
- **Codex 额度**（Codex 的宠物上）：5 小时 / 每周额度用了多少、多久后重置，用到 90% 时提醒。

### 玩法

- agent 干活时钓鱼、工具失败时拍虫子、两个阵营拔河、和历史最佳那天影子赛跑。
- 双人小游戏：羽毛球、乒乓球、掰手腕、猜拳、投球。
- 金币、12 种鱼的鱼类图鉴、20 个成就、只卖外观的商店、周赛季、赛跑下注和擂台。
- 下班时间弹出每日报告，可以复制成图片。

### 自定义

- 在图鉴里按阵营或按项目选宠物；收藏、每日轮换、三档大小。
- 把文件夹或 zip 拖进图鉴、或拖到任意一只宠物身上即可安装宠物包；改了文件自动热加载。
- 自己做宠物包：一个 `pet.json` 加每个动作一张帧条，最少只要一张 `idle`。
- 中英文界面（`language: auto | zh | en`）。

### 隐私与办公室保护

- 只在 `127.0.0.1` 上用 token 通信，不联网，不上传。
- 老板键（`Ctrl+Alt+H`）一键全部隐藏；前台全屏、开会共享屏幕时自动隐身。
- 专注模式只保留状态显示和等你的提醒。

## 快速开始

### 1. 安装

1. 从 Releases 页面下载 `Familiar_<版本>_x64-setup.exe`，或者[自己构建](#从源码构建)。
2. 运行安装包。装到当前用户目录，不需要管理员权限。安装包还没有代码签名，SmartScreen 可能会拦一下，见[常见问题](#常见问题)。
3. 装好后 Familiar 会自己启动，并默认开机自启（托盘 → 开机自启 可以关掉）。
4. 首次启动分两步给 Claude Code 和 Codex 各选一只宠物，可以跳过。

### 2. 接入 Claude Code

托盘 → 接入 → **接入 Claude Code…**

它会替你运行 `claude plugin marketplace add` 和 `claude plugin install familiar@familiar`（需要已经装了 Claude Code 命令行版）。装完重开 Claude Code。

手动操作的话，在普通终端里执行：

```
claude plugin marketplace add /path/to/familiar
claude plugin install familiar@familiar
```

用安装包安装时，marketplace 在 `familiar.exe` 旁边的 `plugin` 文件夹里。Windows 上路径也要用正斜杠（`C:/path/to/familiar`）；在 Claude Code 交互界面里执行 `/plugin marketplace add` 时，反斜杠路径会没有响应。

### 3. 接入 Codex

托盘 → 接入 → **接入 Codex…**

它会在 Codex 的 `hooks.json` 里追加 Familiar 的 hook，原有的 hook 保留，写入前先备份。

**最后还有一步必须手动做：**在终端运行 `codex`，输入 `/hooks`，把 Familiar 的 hook 标记为信任。Codex 只运行信任过的 hook。ChatGPT / Codex 桌面应用里没有 `/hooks` 命令，但两者共用同一个配置目录，在命令行里信任过就行。

两个"接入"都会先弹窗说明要改什么，点"接入"才执行。

手动操作：

```
familiar-emit --install codex
```

- 写到 `$CODEX_HOME/hooks.json`，没设 `CODEX_HOME` 时写到 `~/.codex/hooks.json`。也可以用 `--codex-home <目录>` 指定。
- 你原有的 hook 原样保留，Familiar 的 hook 追加在每个事件的最后。
- 写入前备份为 `hooks.json.familiar-bak`。
- emitter 会复制到 `~/.familiar/bin/`，以后挪动仓库不影响。
- 不确定 Codex 用的是哪个配置目录时（比如设了 `CODEX_HOME`，同时也在用 `~/.codex`），两个都装：再执行一次，加上 `--codex-home %USERPROFILE%\.codex`。
- 卸载：`familiar-emit --uninstall codex`

完成。在任意一个 agent 里开个会话，对应的宠物就动起来了。

## 使用

### 鼠标操作

| 操作 | 效果 |
|---|---|
| 单击 | 捏一下，随机三种音效 |
| 在头上来回摸 3 次 | 撸毛：冒爱心，心情值 +1，每分钟最多 +3 |
| 5 秒内单击超过 12 次 | 生气跳开，说"够了！"，之后 60 秒内再点只会委屈 |
| 按住拖动 | 移动宠物；松手后按最后 80 ms 的速度甩出去，会反弹，撞墙太狠会晕 |
| 双击 | 把会话所在的终端或 IDE 调到前台 |
| 悬停 1 秒 | 会话卡片 |
| 右键 | 圆形菜单 |
| 把一只宠物拖到另一只身上 | 投球 |
| 把宠物包文件夹或 zip 拖到宠物身上 | 安装宠物包 |

宠物会落在屏幕底边或普通窗口的标题栏上，脚下的窗口被移走或关掉时会掉下来。两只宠物不会重叠，叠在一起时会自动弹开。宠物身体以外的透明区域点击穿透，不挡鼠标。

### 右键圆形菜单

右键宠物，周围弹出一圈圆形按钮：**游戏**、**投喂**、**换装**、**设置**、**图鉴**、**报告**、**隐藏**；Codex 的宠物多一个**重置卡**。贴着屏幕边或底边时自动改成半圈或四分之一圈。有子项的按钮点开后在旁边弹出卡片（游戏、猜拳、擂台、下注、大小、路由模式、专注模式、语言、编辑配置……）。点空白处、按 Esc 或切到别的窗口就收起。`/pet menu` 也能打开。

<!-- screenshot：宠物周围的圆形菜单 -->

### 托盘菜单

显示宠物 · 全部隐藏（老板键）· 专注模式 · 宠物图鉴… · 今日报告…（带金币余额）· 商店… · 路由模式 · 语言 · 编辑配置… · 打开数据目录 · 接入（Claude Code / Codex）· 开机自启 · 退出。

有宠物包校验失败时，图鉴那一项会显示出错的包数。

### `/pet` 命令（Claude Code）

| 命令 | 作用 |
|---|---|
| `/pet show`、`/pet hide` | 显示、隐藏宠物 |
| `/pet stats` | 查看各会话状态 |
| `/pet switch [包名]` | 切换当前阵营的宠物 |
| `/pet mode [模式]` | 切换路由模式 |
| `/pet focus [on/off]` | 专注模式 |
| `/pet wallet` | 金币余额和今日收入 |
| `/pet report` | 打开今日报告 |
| `/pet size s/m/l` | 宠物大小 |
| `/pet lang auto/zh/en` | 界面语言 |
| `/pet next` | 在收藏里换下一只 |
| `/pet tug` | 马上来一局拔河 |
| `/pet duel` | 发起擂台 |
| `/pet badminton`、`/pet pingpong`、`/pet armwrestle` | 来一局羽毛球 / 乒乓球 / 掰手腕 |
| `/pet menu [宠物]` | 打开右键圆形菜单 |
| `/pet fling [宠物] [vx] [vy]` | 把宠物甩出去（调试物理用） |

Codex 没有对应的命令，用托盘、右键菜单或图鉴操作。

### 快捷键

| 默认 | 作用 |
|---|---|
| `Ctrl+Alt+H` | 老板键：所有宠物和覆盖层立即隐藏并静音；再按一次，宠物从下面探头回来 |
| `Ctrl+Alt+F` | 专注模式：只保留状态显示和等你的提醒，关闭玩法、互动、吐槽和其他音效 |
| `Ctrl+Alt+P` | 在收藏里切换当前阵营的宠物（当前阵营 = 最近有事件的那一边） |

在 `config.yaml` 的 `hotkeys` 里修改，重启后生效。

### 自动隐身

- **全屏**：前台窗口全屏时（看视频、放幻灯片）自动隐藏，退出全屏 3 秒后恢复。
- **共享屏幕**：检测到腾讯会议、飞书、钉钉、Zoom、Teams 等的"正在共享"工具条窗口时自动隐藏。识别不了你用的会议软件时，在 `config.yaml` 的 `meeting.share_windows` 里加工具条窗口标题或类名的关键词。

### 宠物图鉴

托盘 → **宠物图鉴…**（或右键菜单）：

- 预览所有包，按标签筛选，收藏。
- 给 Claude Code、Codex 或某个项目选宠物；切换路由模式。
- 分页查看鱼类图鉴、钱包与战绩、成就、商店。
- 导入宠物包（文件夹、zip 或直接拖进来）。校验失败的包会列出原因。

切换宠物有个小动画：挥手 → 一团烟 → 新宠物原地出现。处于等你（attention）状态时立即替换，不耽误提醒。

## 玩法与互动

所有玩法都不会干扰状态显示。**只要有会话在等你，所有互动都冻结**（只有跑过去催你的那条例外），虫子也暂停不能拍。每只宠物每小时最多 6 次主动互动。

**钓鱼**：会话干活 5 秒后宠物开始钓鱼。每轮结束收竿，按本轮时长抽鱼：时长越长，越容易钓到稀有、史诗、传说。出错的回合钓到破靴子，被打断的回合鱼跑了。金币：普通 1、稀有 3、史诗 8、传说 20；本轮没有工具失败时 ×1.5。同一小时第 10 条鱼起金币减半，几轮同时结束会合并成一次收竿。短于 5 秒的回合不出鱼。一共 12 种鱼。

**抓 Bug**：每次工具失败，宠物身上掉一只虫；回合出错，掉 3 只。虫子沿屏幕底边爬，单击拍扁，每拍死 5 只得 1 金币。同一轮连续 3 次失败会掉出 Boss：2 倍大，要点 10 下，同时宠物石化；打死 Boss 得 5 金币，石化解除。屏幕上最多 20 只，10 分钟没拍的虫会钻进地缝。

**拔河**（两个阵营都在线时）：由两只主宠物本体来演——Codex 的宠物走到 Claude Code 宠物旁边，两只后仰拉绳约 5 秒，领先的说"我领先！"，落后的被往绳子那边拽，结束后走回原位。效率分 = 一次通过率 − 出错率，与开几个会话无关；每边完成 3 轮以上才有分。两边刚同时在线时先来一局，之后比分有变化、或者距离上一局超过 20 分钟就会再演，冷却 10 分钟。右键"来一局拔河"或 `/pet tug` 随时开，不受冷却限制。按会话模式出了两只同阵营宠物时，它俩也会拔河（同门内卷）。`features.arena: false` 可以关掉。

**影子赛跑**（只有一个阵营在线时）：今天的完成轮数超过历史最佳那天同一时刻时，宠物欢呼。

**双人小游戏**：两只宠物都闲了 2 分钟以上时偶尔自己玩羽毛球、乒乓球、掰手腕（冷却 20 分钟）；右键 → 游戏 或 `/pet badminton|pingpong|armwrestle` 随时开。猜拳：两只都闲了 3 分钟以上会自己玩三局两胜，也可以右键"猜拳"和宠物对战，赢了 +2 金币。投球：把一只宠物拖到另一只身上，两只来回扔 5 次球，球在空中时点它就截下来，+1 金币，冷却 2 分钟。

**下注与擂台**：两边 2 分钟内先后开始新一轮就开跑，20 秒内可以右键下注 10 / 50 / 100 金币（单注不超过余额的一半）。先完成且成功的一方赢，出错或被打断直接判负。赔率 = 0.9 ÷ 最近 20 场的胜率，限制在 1.2–5.0；不足 5 场时都是 1.8。只有一边在线时有庄家局，押这一轮"不到 1 分钟 / 1–5 分钟 / 超过 5 分钟 / 会出错 / 要你确认"。**擂台**（右键或 `/pet duel`）：两边各挑一个会话，各自的下一轮里先一次通过的赢，胜方 +15 金币；完成时间相差不到 5 秒算平局。

**吐槽与日常**：干活时每 90–180 秒随机吐槽一句（比如"又改 main.rs 了，第 5 次"），同一句 30 分钟内不重复。内置互动规则：对方出错时嘲讽、对方石化时安慰、对方完成长任务时鼓掌、对方等你时空闲的宠物跑过去挥手催你。还有打脸、抢活、偷吃、23 点后提醒早点休息、下班前 30 分钟收拾书包、周五 17 点后跳舞、对手好几天没来时问"它今天没来上班？"等。

**金币、商店与养成**：
- 金币全局计算，余额由流水求和得出。每日收入上限 300，花钱不算收入。
- 20 个成就（一次通过、十连胜、传说猎人、Bug 克星、夜猫子……），解锁奖励 10–100 金币。
- 商店只卖外观：7 顶帽子（按宠物分别佩戴，会自动识别每一帧头顶的位置，自制宠物同样适用）、2 种拔河绳、2 个吐槽台词包。
- 周赛季：每周一结算上周拔河效率分，冠军阵营的宠物戴一周皇冠，输家戴"输"字纸帽。
- 投喂（右键）：消耗鱼篓里最新的一条鱼，心情 +3。
- 好感度（两只宠物之间，0–100）：投喂、猜拳、拔河、安慰都会加。20 解锁拔河后击掌，50 解锁并排睡，80 解锁每日报告上的合照。
- 进化：每个宠物包累计完成 100 / 500 / 2000 轮升到 1 / 2 / 3 阶，身后出现蓝 / 紫 / 金色光环（宠物包可以自定义阈值和每一阶的图）。

**Codex 重置卡**（只在 Codex 的宠物上）：右键 → 重置卡，查看 5 小时 / 每周额度各用了多少、多久后重置。"使用重置卡…"会打开一个运行 `codex` 的终端，在里面输入 `/usage` 选择使用（桌面版在 设置 → 用量）。用卡是你账号上的操作，Familiar 不会替你点。检测到额度在到期前突然清空（也就是用了重置卡）时，宠物头上飞来一张金色重置卡，然后欢呼"满血复活"。

**健康提醒**（默认关闭）：`features.water: true` 每 60 分钟空闲时提醒喝水；`features.breathe: true` 时，10 分钟内被叫去确认超过 6 次，宠物带你做 30 秒 4-7-8 呼吸，点一下宠物跳过。

**每日报告**：下班时间（默认 18:00）自动弹出，内容包括 agent 工作时长、完成轮数、钓到的鱼、拍死的虫、拔河结果。可以复制成图片，托盘和右键菜单里也能随时打开。

每轮的明细保留 90 天，更早的汇总成每日数据。

## 配置

运行时数据在 `~/.familiar/`，可以用环境变量 `FAMILIAR_HOME` 改到别处。托盘 → **编辑配置…** 打开 `config.yaml`，保存后自动生效。

```yaml
language: auto           # 界面语言：auto（跟随系统）| zh | en
mode: per-camp           # per-camp | single | per-rule | per-session
max_minis: 4             # per-camp 下每只主宠物最多几个分身
max_pets: 4              # per-session 下最多几只宠物

selection:
  claude-code: pixel-cat
  codex: pixel-bot
  projects: { f1-*: pixel-bot }   # 按项目覆盖；键可以是文件夹名、完整路径或通配符
  favorites: [pixel-cat, pixel-bot]
  rotation: off          # off | daily-random（每天随机一只收藏）| weekday（按星期轮换收藏）

# per-rule 模式下每条规则一只宠物
# pets:
#   - { id: work, pack: pixel-cat, match: { project: "C:/work/*" } }
#   - { id: bots, pack: pixel-bot, match: { source: codex } }

features:
  focus_mode: false
  sounds: true
  arena: true            # 拔河小剧场
  size: m                # s | m | l（0.75 / 1 / 1.35 倍）
  water: false           # 每小时提醒喝水
  breathe: false         # 被频繁叫去确认时一起深呼吸

# meeting:
#   share_windows: ["正在共享"]   # 识别会议软件"正在共享"工具条的额外关键词

hotkeys:
  boss_key: Ctrl+Alt+H
  focus_toggle: Ctrl+Alt+F
  cycle_pet: Ctrl+Alt+P
```

路由模式：

| 模式 | 宠物 |
|---|---|
| `per-camp`（默认） | 每个阵营（Claude Code / Codex）一只主宠物，每个会话一个小分身 |
| `single` | 只有一只宠物 |
| `per-rule` | `pets:` 里每条规则一只宠物（按 `source` 和/或 `project` 匹配） |
| `per-session` | 每个会话一只宠物（最多 `max_pets` 只） |

图鉴、右键菜单和 `/pet switch` 会改写这个文件：你自己加的配置项会保留，**注释不会保留**（YAML 库的限制）。写入前会先校验，不合法的修改不会落盘。

### 数据目录

| 路径 | 内容 |
|---|---|
| `config.yaml` | 配置 |
| `token`、`port` | 本地接口的凭据和端口 |
| `spool/` | Familiar 没运行时暂存的事件 |
| `pets/` | 自己安装的宠物包 |
| `windows.json` | 宠物位置（显示器拔掉后回到主屏右下角） |
| `familiar.db` | 回合记录、钱包流水、鱼篓、拍虫记录、心情值、互动日志（SQLite） |
| `balance.yaml`、`fish.yaml` | 可选：覆盖内置的数值和鱼种，只写要改的键（比如 `work: { off_time: "19:00" }` 改下班时间） |
| `interactions/*.yaml` | 可选：自己的互动规则；同 id 覆盖内置规则，`enabled: false` 可关闭 |
| `bin/` | 安装到 Codex 的 emitter |
| `logs/petd.log` | 日志 |

内置默认值在 [`crates/petd/assets/`](crates/petd/assets/)（`balance.yaml`、`fish.yaml`、`interactions.yaml`），可用字段写在文件注释里。

## 自制宠物包

宠物包就是一个文件夹：

```
my-dog/
  pet.json
  sprites/idle.png  sprites/tool.png  …     每个动作一张横排帧条 PNG（透明背景）
  sounds/attention.wav  …                    可选，wav / mp3 / ogg
```

### pet.json

```json
{
  "id": "my-dog",
  "name": "我的小狗",
  "name_en": "My Dog",
  "version": "1.0.0",
  "author": "你的名字",
  "description": "一句话介绍",
  "description_en": "One-line description",
  "frame": { "w": 64, "h": 64, "scale": 2 },
  "actions": {
    "idle":      { "sheet": "sprites/idle.png", "frames": 4, "fps": 4, "loop": true },
    "tool":      { "sheet": "sprites/tool.png", "frames": 4, "fps": 8, "loop": true },
    "attention": { "sheet": "sprites/attention.png", "frames": 2, "fps": 4, "loop": true, "sound": "sounds/attention.wav" },
    "done":      { "sheet": "sprites/done.png", "frames": 6, "fps": 10, "loop": false, "then": "idle" },
    "stone":     { "fallback": "error", "tint": "#9aa0a6" }
  },
  "bubbles":    { "attention": ["汪！该你了"], "done": ["汪汪，搞定"] },
  "bubbles_en": { "attention": ["Woof! Your turn"], "done": ["Woof, done"] },
  "sounds":  { "squish": ["sounds/s1.wav", "sounds/s2.wav"] },
  "evolutions": [{ "turns": 100, "dir": "stage1" }, { "turns": 500, "dir": "stage2" }, { "turns": 2000 }],
  "tags": ["狗", "可爱"],
  "tags_en": ["dog", "cute"]
}
```

| 字段 | 说明 |
|---|---|
| `id` | 唯一标识，也是安装后的文件夹名 |
| `name`、`description`、`tags` | 显示名、一句话介绍、图鉴标签 |
| `name_en`、`description_en`、`tags_en` | 可选的英文版本，界面切到英文时使用；缺了或为空就用上面的中文 |
| `version`、`author` | 显示在图鉴里；导入覆盖同 id 的包时会比较版本号 |
| `frame` | `w`、`h`：帧尺寸（像素），所有动作共用；`scale`：显示倍数 |
| `actions.<动作名>` | `sheet`（帧条）、`frames`、`fps`、`loop`、`then`、`sound`、`fallback`、`tint` |
| `bubbles` | 各状态的气泡台词（`attention`、`done`、`error`、`stone`……），随机挑一句 |
| `bubbles_en` | 英文台词，格式相同；按键合并，这里缺的状态继续用中文 |
| `sounds.squish` | 可以放多个文件，捏的时候随机播一个 |
| `evolutions` | 可选，见下文 |

- `loop: false` 的是一次性动作，播完按 `then` 切走；不写 `then` 就回到当前状态对应的动作。
- `fallback`：没画这个动作时借用哪个动作。`tint`：借用时叠一层颜色。
- `evolutions`：每一项的 `turns` 是升到这一阶需要的累计轮数；`dir` 是这一阶的图所在的子文件夹，里面放和 `sprites/` 同名的帧条，缺哪张就用基础的那张。不写 `evolutions` 时阈值是 100 / 500 / 2000，只加光环不换图。

### 动作清单

| 优先级 | 动作 | 没画时用 |
|---|---|---|
| 必需 | `idle` | — |
| 强烈建议 | `thinking`、`tool`、`attention`、`done`、`error`、`sleep` | idle |
| 建议 | `reading`（→ tool）、`wake`、`wave`（切换时挥手）、`squish`（被捏） | 括号里的动作，或 idle |
| 玩法用 | `fish`、`fish_reel`、`walk`、`fall`、`dizzy`、`petted`、`annoyed`、`sulk`、`cheer`、`taunt`、`comfort`、`peek`、`eat`（投喂）、`pack_bag`（下班前收拾书包） | idle（玩法照常，只是动作不生动） |
| 可选 | `stone`、`aborted`（→ error）、`celebrate`（→ done） | 括号里的动作；可以加 `tint` 染色 |

没画的动作会按上表自动借用别的动作，所以最小的宠物包只需要一张 idle 帧条。

### 校验规则

导入时会校验，不通过就列出原因，不会安装。

- `id` 不能为空，不能含 `/`、`\`、`.`。
- 必须有带图的 `idle`。
- 每张帧条：宽 = 帧数 × `frame.w`，高 = `frame.h`。所有动作使用同一帧尺寸。
- 引用的图片、音效文件都要存在。
- `fallback` 不能绕成环，`then` 要指向存在的动作。

### 导入

- 图鉴 → **导入文件夹…** 或 **导入 zip…**；也可以把文件夹或 zip 直接拖进图鉴窗口，或拖到任意一只宠物身上。
- 已有同 id 的宠物包时，先显示新旧版本号，问你是否覆盖。覆盖内置宠物不会删掉内置版本，删掉导入的这份就会恢复。
- zip 里的文件可以放在根目录，也可以套一层文件夹。
- 宠物包存放在 `~/.familiar/pets/<id>/`。直接把文件夹复制到那里也行，约 2 秒内自动加载，改了图片或 `pet.json` 也会自动重新加载。
- 自己导入的宠物卡片右上角有"删除"按钮，点两次确认。正在被某个阵营、项目或规则使用的宠物包，删除后会自动换回默认宠物。

### 作画建议

- 宠物的脚画在帧的最底一行。落地、站在窗口标题栏上、两只宠物拉绳对齐，都以帧底边为准。
- 显示大小约 100 px 比较合适：32×32 的像素画用 `scale: 3`，64×64 用 2，128×128 用 1。
- 两个阵营选了同一只宠物时，Codex 那只会自动转 180° 色相，不用另画。
- 参考内置宠物 [`pets/pixel-cat/`](pets/pixel-cat/)，生成脚本在 [`tools/gen-builtin-pets.py`](tools/gen-builtin-pets.py)。

### 从一张图生成宠物（可选）

[`tools/petgen/`](tools/petgen/README.md) 是一条示例流水线，用图片模型加图生视频模型，把一张角色图做成宠物包：

1. 用图片模型从原图生成一张纯绿背景的 Q 版全身立绘。
2. 以立绘为首帧，用图生视频模型给每个动作生成一段短视频——idle、thinking、tool、attention、done、error、sleep（镜头固定）。
3. `build_pack.py` 用 ffmpeg 抽帧、绿幕抠像、挑出首尾接得上的循环片段、裁边，打包成带 `pet.json` 的标准宠物包，并输出一张效果总览图方便检查。

图片和视频模型用你自己的就行：在工作目录里产出 `idle.mp4`、`thinking.mp4`……，再跑第 3 步。好用的提示词见 [`tools/petgen/prompts.md`](tools/petgen/prompts.md)，步骤和经验见 [`tools/petgen/README.md`](tools/petgen/README.md)。

**角色版权**：用别人的角色做的宠物包自己用没问题；分享或再发布需要版权方授权。

## 从源码构建

需要 Rust 1.88+、Node 18+、WebView2（Windows 10/11 自带）。

```bash
cd app && npm install && cd ..
node tools/build.mjs            # release；加 --debug 编得更快
```

这一步会构建 `target/release/familiar.exe`，并把 `familiar-emit` 复制到 `adapters/claude-code/bin/`。

出安装包：`node tools/build.mjs --bundle`，生成 `target/release/bundle/nsis/Familiar_<版本>_x64-setup.exe`。安装包里带着内置宠物、Claude Code 插件和 emitter。第一次构建时 Tauri 会下载 NSIS。

npm 里的 `@tauri-apps/*` 版本要和 Rust 的 Tauri 版本保持同一个小版本（目前都是 2.11 / dialog 2.7），否则 `tauri build` 会拒绝打包。

### 从源码运行

1. 手动运行一次 `familiar.exe`。它会把自己的路径写进 `~/.familiar/petd_path`，以后 emitter 发现它不在时会自动拉起。
2. 以仓库作为 marketplace 接入 Claude Code：
   ```
   claude plugin marketplace add /path/to/familiar
   claude plugin install familiar@familiar
   ```
3. 接入 Codex：`adapters\claude-code\bin\familiar-emit.exe --install codex`，然后在 `codex` 里用 `/hooks` 信任。

Claude Code 会把插件复制到 `~/.claude/plugins/cache/`。重新构建后要把 `adapters/claude-code/.claude-plugin/plugin.json` 的版本号改掉，更新插件，然后重开 Claude Code：

```
claude plugin marketplace update familiar
claude plugin update familiar@familiar
```

修改内置宠物：编辑 `tools/gen-builtin-pets.py`，然后运行 `python tools/gen-builtin-pets.py`（需要 Pillow）。

测试、调试方法和贡献规范见 [CONTRIBUTING.md](CONTRIBUTING.md)；实现说明见 [docs/implementation-notes.md](docs/implementation-notes.md)。

## 目录结构

```
crates/emit/              hook 发射器（Rust，单文件可执行），含 Codex 安装器
crates/petd/              守护进程核心库：状态机、路由、宠物包、配置、HTTP/WS、世界层、SQLite
crates/petd/assets/       内置的 balance.yaml（数值）、fish.yaml（鱼种）、interactions.yaml（互动规则）
app/                      Tauri 2 外壳 + TypeScript/Canvas 页面
  index.html              宠物窗口（每只宠物一个）
  card.html               悬停会话卡片
  gallery.html            宠物图鉴
  welcome.html            首次启动选择
  overlay.html            覆盖层（每个显示器一个）：虫子、拔河绳、赛跑条
  report.html             每日报告卡片
adapters/claude-code/     Claude Code 插件：hooks/hooks.json、commands/pet.md、bin/
adapters/codex/           Codex 映射表 mapping.yaml
pets/                     内置宠物包（pixel-cat、pixel-bot）
tools/                    构建脚本、内置宠物生成器、petgen
docs/                     实现说明
```

工作原理：agent 的每个 hook 都会运行 `familiar-emit`，它把一个小事件发给本地守护进程（`petd`，跑在 App 里面）后立刻退出——总耗时 ≤ 300 ms，退出码恒为 0，不写 stdout，所以永远不会卡住或弄坏你的 agent。App 没在运行时，事件先写进 spool，再在后台拉起 App。守护进程为每个会话维护一个状态机，把会话分配给宠物，通过本地 WebSocket 把状态推给宠物窗口。

## 隐私与安全

- **只在本机。**守护进程只监听 `127.0.0.1`（默认端口 47321，被占用时随机选一个空闲端口），每个请求都要带 `~/.familiar/token` 里的 token。Familiar 不发任何网络请求，不上传任何东西，所有数据都在 `~/.familiar/`。
- **事件只带必要信息。**hook 只把宠物需要的东西交给守护进程：会话 id、事件类型、会话的工作目录、工具名、被修改文件的文件名和 agent 进程号。提示词、文件内容、工具输出都不会转发。Codex 的 shell 命令只看第一个词来判断是不是读取类操作，命令原文不离开 emitter。
- **hook 只追加，不替换。**Claude Code 这边是一个普通插件。Codex 那边，Familiar 把自己的 hook 追加在你的后面，写入前先备份 `hooks.json`；`--uninstall codex` 可以移除。两个"接入"都会先告诉你要改什么。
- **Codex 额度只读。**额度显示读取的是 Codex 自己的会话日志（`$CODEX_HOME/sessions` 和 `~/.codex/sessions`），不做任何修改。Familiar 永远不会替你使用重置卡。
- **本机系统信息。**为了检测关掉的会话、跳转到终端、全屏和共享屏幕时隐身，App 会读取本机的进程列表和窗口标题。这些信息不保存，也不发往任何地方。

## 常见问题

**Windows SmartScreen 拦截了安装包。**
安装包还没有代码签名。点"更多信息 → 仍要运行"，或者自己构建安装包。

**Codex 的宠物一直不动。**
Codex 只运行信任过的 hook。在终端运行 `codex`，输入 `/hooks`，把 Familiar 的 hook 标记为信任。如果你用了不止一个 Codex 配置目录，每个都要装（见[接入 Codex](#3-接入-codex)）。

**重新构建后 Claude Code 还在跑旧的 emitter。**
Claude Code 运行的是缓存里的插件副本。改掉插件版本号，执行 `claude plugin marketplace update familiar` 和 `claude plugin update familiar@familiar`（或者再点一次 托盘 → 接入 Claude Code…，已安装时会自动更新），然后重开 Claude Code。

**宠物不见了。**
依次检查：是不是按了老板键（`Ctrl+Alt+H`）；前台是不是有全屏窗口；会议软件是不是在共享屏幕；是不是执行过 `/pet hide`（用 `/pet show` 或 托盘 → 显示宠物 恢复）。

**共享屏幕时没有隐身。**
在 `config.yaml` 的 `meeting.share_windows` 里加上会议软件"正在共享"工具条的窗口标题或类名关键词。识别靠的是这些工具条，内置名单不保证认得每个版本。

**宠物包没有出现。**
打开图鉴：校验失败的包会在那里列出原因（托盘上也会显示出错的包数）。

**双击跳不到具体的标签页。**
Windows Terminal 只能切到窗口，切不到具体标签页。

**日志在哪？**
`~/.familiar/logs/petd.log`。

## 路线图

- macOS 支持（尚未开始）。
- 安装包代码签名。
- 投球专用的接球、扔球帧（内置宠物目前用挥手和欢呼代替）。
- Codex 宠物包导入器：需要先拿到真实的源格式样本，并确认授权。
- 覆盖更多会议软件的共享屏幕检测。

## 许可证

[MIT](LICENSE)。内置宠物（像素猫、像素机器人）及其音效都是仓库里的代码生成的，同样适用 MIT 许可证。

用别人的角色做的宠物包仅限自己使用；分享需要版权方授权。
