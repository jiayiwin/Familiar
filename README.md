# Familiar

[中文文档](README.zh-CN.md)

Familiar puts small desktop pets on your screen that show what your coding agents are really doing. Each Claude Code and Codex session drives a pet: it thinks while the agent thinks, types while tools run, waves at you the moment the agent needs your approval, celebrates when a turn finishes and slumps when something fails. Between the work there is a light layer of play — fishing, bug squashing, tug-of-war and mini games between the Claude Code pet and the Codex pet — plus a gallery, a daily report and custom pet packs. Everything runs locally on your machine: no account, no server, no telemetry.

Windows 10/11 only for now. Built with Rust, Tauri 2 and TypeScript/Canvas.

<!-- screenshot: the two team pets on the desktop, one waving for attention -->

## Contents

- [Features](#features)
- [Quick start](#quick-start)
- [Using Familiar](#using-familiar)
- [Games and interactions](#games-and-interactions)
- [Configuration](#configuration)
- [Making your own pet pack](#making-your-own-pet-pack)
- [Build from source](#build-from-source)
- [Project layout](#project-layout)
- [Privacy and security](#privacy-and-security)
- [FAQ and troubleshooting](#faq-and-troubleshooting)
- [Roadmap](#roadmap)
- [License](#license)

## Features

### Status at a glance

- **Real session state**, taken from the agents' own hooks: thinking, reading, using tools, waiting for you, done, error, interrupted, asleep.
- **One pet per team, one mini per session** (default `per-camp` mode). Sessions waiting for you line up closest to the main pet with an exclamation mark and the project name under their feet. Other routing modes: `single`, `per-rule`, `per-session`.
- **Team labels**: an orange "Claude" tag and a teal "Codex" tag under each team pet. If both teams use the same pack, the Codex one is hue-shifted 180°.
- **Hover for one second** to see a session card: project, agent, state, running time and the latest tool.
- **Double-click** a pet to bring the terminal or IDE of that session to the front (the main pet jumps to the session that needs you most).
- **Dead sessions disappear**: agent processes are checked every 5 seconds, so closing a terminal removes its mini within seconds.
- **Codex quota** (on the Codex pet): how much of the 5-hour and weekly limits you have used, when they reset, and a heads-up at 90%.

### Games and fun

- Fishing while the agent works, bugs to squash when tools fail, tug-of-war between the two teams, shadow races against your best day.
- Two-pet games: badminton, ping-pong, arm wrestling, rock-paper-scissors, ball toss.
- Coins, a 12-fish collection, 20 achievements, a cosmetics-only shop, weekly seasons, race betting and duels.
- A daily report at the end of the workday, which you can copy as an image.

### Customization

- Pick a pet per team or per project in the gallery; favorites, daily rotation, three sizes.
- **Make a pet from any picture** right in the gallery: pick or drop an image, name it, preview all 16 actions, install. The background is removed automatically; nothing else to install ([tutorial](docs/make-a-pet.md)).
- Install pet packs by dragging a folder or zip onto the gallery or onto any pet. Packs hot-reload when you edit them.
- Write your own pack: a `pet.json` plus one sprite strip per action. Only `idle` is required.
- Chinese and English UI (`language: auto | zh | en`).

### Privacy and office-friendliness

- Everything stays on `127.0.0.1` behind a token. No network calls, no uploads.
- Boss key (`Ctrl+Alt+H`) hides everything instantly. Pets also hide automatically while a fullscreen app is in front or while you are sharing your screen in a meeting.
- Focus mode keeps only status and "waiting for you" alerts.

## Quick start

### 1. Install

1. Download `Familiar_<version>_x64-setup.exe` from the Releases page, or [build it yourself](#build-from-source).
2. Run it. It installs into your user profile and needs no administrator rights. The installer is not code-signed yet, so SmartScreen may warn you — see the [FAQ](#faq-and-troubleshooting).
3. Familiar starts by itself and is set to start with Windows (Tray → **Start with Windows** turns that off).
4. On first launch you pick a pet for Claude Code and one for Codex. You can skip this.

### 2. Connect Claude Code

Tray → **Hook up** → **Hook up Claude Code…**

This runs `claude plugin marketplace add` and `claude plugin install familiar@familiar` for you (the Claude Code CLI must be installed). Restart Claude Code afterwards.

Manual equivalent, in a normal terminal:

```
claude plugin marketplace add /path/to/familiar
claude plugin install familiar@familiar
```

When the installer is used, the marketplace lives in the `plugin` folder next to `familiar.exe`. Use forward slashes in the path on Windows too (`C:/path/to/familiar`); a backslash path in Claude Code's interactive `/plugin marketplace add` silently does nothing.

### 3. Connect Codex

Tray → **Hook up** → **Hook up Codex…**

This appends Familiar hooks to Codex's `hooks.json`. Your existing hooks are kept, and the file is backed up first.

**Then one more step is required:** open a terminal, run `codex`, type `/hooks` and mark the Familiar hooks as trusted. Codex only runs trusted hooks. The ChatGPT / Codex desktop app has no `/hooks` command, but it shares the same config folder, so trusting the hooks once in the CLI is enough.

Both "Hook up" actions show what they are about to change and only run after you click **Hook up**.

Manual equivalent:

```
familiar-emit --install codex
```

- Writes to `$CODEX_HOME/hooks.json`, or `~/.codex/hooks.json` when `CODEX_HOME` is not set. Use `--codex-home <dir>` to pick a folder.
- Your hooks stay as they are; Familiar' hook is appended at the end of each event.
- The original is backed up as `hooks.json.familiar-bak`.
- The emitter is copied to `~/.familiar/bin/`, so moving the repo later does not break anything.
- Not sure which config folder Codex uses (for example `CODEX_HOME` is set but you also use `~/.codex`)? Install into both: run it again with `--codex-home %USERPROFILE%\.codex`.
- Uninstall: `familiar-emit --uninstall codex`.

That's it. Start a session in either agent and its pet comes to life.

## Using Familiar

### Mouse

| Action | What happens |
|---|---|
| Click | Squish (one of three sounds) |
| Stroke the head back and forth 3 times | Petting: hearts, mood +1 (at most +3 per minute) |
| More than 12 clicks in 5 seconds | The pet gets annoyed, says "Enough!" and jumps away; for 60 seconds more clicks only make it sulk |
| Press and drag | Move the pet. Let go and it flies off with your last 80 ms of speed, bounces, and may get dizzy hitting a wall hard |
| Double-click | Bring the session's terminal or IDE to the front |
| Hover 1 second | Session card |
| Right-click | Ring menu |
| Drag one pet onto the other | Ball toss |
| Drop a pack folder or zip onto a pet | Install the pack |

Pets land on the bottom of the screen or on the title bar of a normal window, and fall when that window moves or closes. Two pets never overlap: they push each other apart. Transparent areas around a pet are click-through.

### Right-click ring menu

Right-clicking a pet opens a ring of round buttons around it: **Games**, **Feed**, **Outfits**, **Settings**, **Gallery**, **Report**, **Hide** — and **Reset card** on the Codex pet. Near a screen edge or the bottom it becomes a half or quarter ring. Buttons with sub-items open a card beside the ring (games, rock-paper-scissors, duels, bets, size, routing mode, focus mode, language, edit config…). Click elsewhere, press Esc or switch windows to close it. `/pet menu` opens it too.

<!-- screenshot: the ring menu around a pet -->

### Tray menu

Show pets · Hide all (boss key) · Focus mode · Gallery… · Today's report… (with your coin balance) · Shop… · Routing mode · Language · Edit config… · Open data folder · Hook up (Claude Code / Codex) · Start with Windows · Quit.

The gallery entry shows how many packs failed validation, if any.

### `/pet` commands (Claude Code)

| Command | What it does |
|---|---|
| `/pet show`, `/pet hide` | Show / hide the pets |
| `/pet stats` | State of each session |
| `/pet switch [pack]` | Switch the current team's pet |
| `/pet mode [mode]` | Switch routing mode |
| `/pet focus [on/off]` | Focus mode |
| `/pet wallet` | Coin balance and today's income |
| `/pet report` | Open today's report |
| `/pet size s/m/l` | Pet size |
| `/pet spring` | Open the Spring Festival countdown and the red-envelope game |
| `/pet kegel [times… / off]` | Do a kegel set with your pet now; with times, remind daily, e.g. `/pet kegel 10:30 15:30` |
| `/pet lang auto/zh/en` | UI language |
| `/pet next` | Next pet from your favorites |
| `/pet tug` | Start a tug-of-war now |
| `/pet duel` | Start a duel |
| `/pet badminton`, `/pet pingpong`, `/pet armwrestle` | Play a round of badminton / ping-pong / arm wrestling |
| `/pet menu [pet]` | Open the ring menu |
| `/pet fling [pet] [vx] [vy]` | Throw a pet (physics debugging) |

Codex has no equivalent commands; use the tray, the ring menu or the gallery.

### Hotkeys

| Default | Action |
|---|---|
| `Ctrl+Alt+H` | Boss key: hide all pets and overlays and mute them. Press again and the pets peek back up from below |
| `Ctrl+Alt+F` | Focus mode: keep status and "waiting for you" alerts; turn off games, interactions, quips and other sounds |
| `Ctrl+Alt+P` | Cycle the current team's pet through your favorites (current team = the one with the latest event) |

Change them under `hotkeys` in `config.yaml`; a restart is needed.

### Hiding automatically

- **Fullscreen**: when the foreground window is fullscreen (videos, slides), pets hide and come back 3 seconds after you leave fullscreen.
- **Screen sharing**: when the "you are sharing" toolbar of Tencent Meeting, Feishu/Lark, DingTalk, Zoom, Teams and similar apps is detected, pets hide. If your meeting app is not recognized, add a word from its toolbar's window title or class name to `meeting.share_windows` in `config.yaml`.

### Gallery

Tray → **Gallery…** (or the ring menu):

- Preview every pack, filter by tag, mark favorites.
- Choose a pet for Claude Code, Codex or a specific project; switch routing mode.
- Tabs for the fish collection, wallet & records, achievements and the shop.
- Import packs (folder, zip or drag-and-drop). Packs that fail validation are listed with the reason.

Switching pets plays a little show: the old pet waves, a puff of smoke, the new one appears in the same spot. If the pet is currently asking for your attention, it switches instantly so the alert is not delayed.

## Games and interactions

None of this gets in the way of the status display. **While any session is waiting for you, all interactions freeze** (except the pet that runs over to nudge you), and bugs pause. Each pet starts at most 6 interactions per hour.

**Fishing.** After a session has worked for 5 seconds its pet starts fishing. When the turn ends it reels in a fish based on how long the turn took: longer turns are more likely to give rare, epic or legendary fish. Errored turns catch an old boot; interrupted turns lose the fish. Coins: common 1, rare 3, epic 8, legendary 20, ×1.5 when no tool failed in that turn. From the 10th fish within an hour coins are halved. Turns shorter than 5 seconds catch nothing. 12 species to collect.

**Bug squashing.** Each tool failure drops a bug from the pet; an errored turn drops 3. Bugs crawl along the bottom of the screen; click to squash, 1 coin per 5 bugs. Three failures in a row in one turn spawn a boss bug (twice as big, 10 clicks) and turn the pet to stone until you beat it (5 coins). At most 20 bugs on screen; bugs left alone for 10 minutes crawl away.

**Tug-of-war** (both teams online). The two main pets act it out: the Codex pet walks over, both lean back on a rope for about 5 seconds, the leader brags and the one behind gets dragged. Score = first-pass rate − error rate, independent of how many sessions you run; each side needs at least 3 finished turns. It plays when both teams first come online, then when the score changes or after 20 minutes (10-minute cooldown). Ring menu or `/pet tug` starts one any time. Two pets of the same team (per-session modes) tug over their sessions' scores too. Turn it off with `features.arena: false`.

**Shadow race** (one team online). When today's finished turns pass what you had done at the same time on your best day, the pet cheers.

**Two-pet mini games.** When both pets have been idle for 2 minutes they occasionally play badminton, ping-pong or arm wrestling on their own (20-minute cooldown); the ring menu or `/pet badminton|pingpong|armwrestle` starts one any time. Rock-paper-scissors: best of three on their own after 3 idle minutes, or play against a pet from the ring menu (+2 coins for a win). Ball toss: drag one pet onto the other and they throw a ball back and forth 5 times; click the ball mid-air to steal it (+1 coin, 2-minute cooldown).

**Bets and duels.** When both teams start a new turn within 2 minutes of each other, a race begins; for 20 seconds you can bet 10 / 50 / 100 coins (at most half your balance) from the ring menu. First to finish successfully wins; an error or interrupt loses. Odds = 0.9 ÷ win rate over the last 20 races, clamped to 1.2–5.0 (1.8 until there are 5 races). With only one team online, the bookmaker lets you bet on how the next turn goes (under 1 minute / 1–5 minutes / over 5 minutes / errors / needs your approval). **Duel** (ring menu or `/pet duel`): each side picks a session; the first to pass its next turn without errors wins +15 coins; finishing within 5 seconds of each other is a draw.

**Small talk.** Quips every 90–180 seconds while working ("Editing main.rs again, 5th time"), never repeating within 30 minutes. Built-in interaction rules: taunt when the other side errors, comfort when it turns to stone, applause after a long task, and a nudge where an idle pet runs over to wave when the other side is waiting for you. Plus comebacks, pets sulking when the other team starts a session, fish stealing, a late-night "go to bed" after 23:00, packing a bag 30 minutes before the end of the day, dancing on Friday after 17:00, and a "where were you?" when the other team has been away for days.

**Coins, shop and progress.**
- Coins are global; the balance is the sum of the ledger. Daily income is capped at 300 (spending does not count).
- 20 achievements (first pass, ten in a row, legend hunter, bug buster, night owl…), each paying 10–100 coins.
- The shop sells cosmetics only: 7 hats (worn per pet; the head position is detected on every frame, so hats work on custom packs too), 2 tug ropes, 2 quip packs.
- Weekly seasons: every Monday the previous week's tug scores are added up; the winning team wears a crown for a week, the loser a paper hat.
- Feeding (ring menu): uses the newest fish in your basket, mood +3.
- Affinity between the two pets (0–100) grows with feeding, games, tug and comfort: 20 unlocks a high five after tug, 50 sleeping side by side, 80 a group photo on the daily report.
- Evolution: after 100 / 500 / 2000 finished turns per pack, a pet reaches stage 1 / 2 / 3 and gets a blue / purple / gold aura (packs can define their own thresholds and art).

**Codex reset card** (Codex pet only). Ring menu → **Reset card** shows how much of the 5-hour and weekly quota is used and when each resets. **Use reset card…** opens a terminal running `codex` where you type `/usage` to choose to use it (desktop app: Settings → Usage). Using a card is an action on your account; Familiar never does it for you. When a quota is cleared before its reset time (a card was used), a golden card flies in and the pet cheers.

**Wellbeing** (off by default). `features.water: true` reminds you to drink water every 60 minutes when idle. `features.breathe: true`: if you are called to approve more than 6 times in 10 minutes, the pet leads 30 seconds of 4-7-8 breathing (click to skip).

**Spring Festival**: right-click a pet → Games → Spring Festival (or `/pet spring`) opens a countdown to Lunar New Year with background music: put your own copy of Andy Lau's "恭喜发财" (or any song) in `~/.familiar/music/` ("Music folder" on the page opens it) or use "Pick a song…". An `.lrc` file with the same name shows the lyrics in time with the song. The song is copyrighted, so it is not bundled; without one, an original gong-and-drum tune plays. Below it is a 30-second red-envelope rain: click envelopes and gold ingots, avoid the firecrackers. The first 3 rounds a day pay up to 15 coins each. On New Year's Eve evening and New Year's Day the pet wishes you a happy new year.

**Kegel reminders**: add daily times in Gallery → Reminders (or right-click a pet → Health → Reminder settings). At each time the pet turns its back to you and leads a set: squeezing and lifting with its tail up, then relaxing with a wag, with "Squeeze ⬆ / Relax ⬇", a countdown and a soft beep; the page explains how to do it. Right-click → Health → Kegel set, or `/pet kegel`, starts one any time.

**Daily report.** Pops up at the end of the workday (18:00 by default): agent working time, finished turns, fish caught, bugs squashed, tug results. Copy it as an image, or open it any time from the tray or ring menu.

Per-turn details are kept for 90 days, then rolled up into daily totals.

## Configuration

Runtime data lives in `~/.familiar/` (set `FAMILIAR_HOME` to move it). Tray → **Edit config…** opens `config.yaml`; changes apply as soon as you save.

```yaml
language: auto           # UI language: auto (follow Windows) | zh | en
mode: per-camp           # per-camp | single | per-rule | per-session
max_minis: 4             # per-camp: max minis per main pet
max_pets: 4              # per-session: max pets

selection:
  claude-code: pixel-cat
  codex: pixel-bot
  projects: { f1-*: pixel-bot }   # per-project override; key = folder name, full path or wildcard
  favorites: [pixel-cat, pixel-bot]
  rotation: off          # off | daily-random (random favorite each day) | weekday (rotate favorites by weekday)

# per-rule mode: one pet per rule
# pets:
#   - { id: work, pack: pixel-cat, match: { project: "C:/work/*" } }
#   - { id: bots, pack: pixel-bot, match: { source: codex } }

features:
  focus_mode: false
  sounds: true
  arena: true            # tug-of-war scenes
  size: m                # s | m | l  (0.75 / 1 / 1.35×)
  water: false           # hourly water reminder
  breathe: false         # breathing exercise when you are called too often
  kegel: []              # daily kegel reminder times, e.g. ["10:30", "15:30"]
  kegel_reps: 10         # reps per set (squeeze 3s, relax 3s)

# meeting:
#   share_windows: ["Sharing"]   # extra words to detect your meeting app's sharing toolbar

hotkeys:
  boss_key: Ctrl+Alt+H
  focus_toggle: Ctrl+Alt+F
  cycle_pet: Ctrl+Alt+P
```

Routing modes:

| Mode | Pets |
|---|---|
| `per-camp` (default) | One main pet per team (Claude Code / Codex), one mini per session |
| `single` | Just one pet |
| `per-rule` | One pet per rule in `pets:` (match by `source` and/or `project`) |
| `per-session` | One pet per session (up to `max_pets`) |

The gallery, the ring menu and `/pet switch` rewrite `config.yaml`. Keys you added yourself are kept, but **comments are not** (a limitation of the YAML library). Edits are validated first; an invalid change is never written.

### Data folder

| Path | Contents |
|---|---|
| `config.yaml` | Configuration |
| `token`, `port` | Credentials and port of the local API |
| `spool/` | Events buffered while the app was not running |
| `pets/` | Pet packs you installed |
| `windows.json` | Pet positions (a pet whose monitor is gone returns to the bottom-right of the main screen) |
| `familiar.db` | Turns, coin ledger, fish basket, bugs, mood, interaction log (SQLite) |
| `balance.yaml`, `fish.yaml` | Optional: override built-in numbers and fish; write only the keys you change (e.g. `work: { off_time: "19:00" }` moves the daily report) |
| `interactions/*.yaml` | Optional: your own interaction rules; the same `id` overrides a built-in rule, `enabled: false` turns it off |
| `bin/` | The emitter installed for Codex |
| `logs/petd.log` | Log |

The built-in defaults are in [`crates/petd/assets/`](crates/petd/assets/) (`balance.yaml`, `fish.yaml`, `interactions.yaml`), with the available fields documented in comments.

## Making your own pet pack

A pack is a folder:

```
my-dog/
  pet.json
  sprites/idle.png  sprites/tool.png  …   one horizontal frame strip per action (transparent PNG)
  sounds/attention.wav  …                  optional: wav / mp3 / ogg
```

### pet.json

```json
{
  "id": "my-dog",
  "name": "我的小狗",
  "name_en": "My Dog",
  "version": "1.0.0",
  "author": "Your name",
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

| Field | Meaning |
|---|---|
| `id` | Unique id; also the install folder name |
| `name`, `description`, `tags` | Display name, one-line description, gallery tags |
| `name_en`, `description_en`, `tags_en` | Optional English versions, used when the UI is in English; missing or empty ones fall back to the fields above |
| `version`, `author` | Shown in the gallery; the version is compared when you import over an existing pack |
| `frame` | `w`, `h`: frame size in pixels, shared by all actions. `scale`: display multiplier |
| `actions.<name>` | `sheet` (frame strip), `frames`, `fps`, `loop`, `then`, `sound`, `fallback`, `tint` |
| `bubbles` | Speech-bubble lines per state (`attention`, `done`, `error`, `stone`…); one is picked at random |
| `bubbles_en` | English lines, same shape; merged key by key, so a state missing here keeps its Chinese lines |
| `sounds.squish` | Several files; one is picked at random when the pet is squished |
| `evolutions` | Optional; see below |

- `loop: false` makes a one-shot action. When it ends it switches to `then`, or back to the action for the current state if `then` is missing.
- `fallback`: which action to borrow when this one has no art. `tint`: a color laid over the borrowed action.
- `evolutions`: each entry's `turns` is the number of finished turns needed for that stage; `dir` is a subfolder holding strips with the same names as in `sprites/` (any missing strip uses the base one). Without `evolutions` the thresholds are 100 / 500 / 2000 and only the aura is added.

### Actions

| Priority | Actions | Falls back to |
|---|---|---|
| Required | `idle` | — |
| Strongly recommended | `thinking`, `tool`, `attention`, `done`, `error`, `sleep` | idle |
| Recommended | `reading` (→ tool), `wake`, `wave` (when switching pets), `squish` (when clicked) | the action in brackets, or idle |
| Games | `fish`, `fish_reel`, `walk`, `fall`, `dizzy`, `petted`, `annoyed`, `sulk`, `cheer`, `taunt`, `comfort`, `peek`, `eat` (feeding), `pack_bag` (end of day) | idle (the game still works, it just looks less lively) |
| Optional | `stone`, `aborted` (→ error), `celebrate` (→ done) | the action in brackets; add `tint` to color it |

Missing actions borrow from others automatically, so the smallest valid pack is one idle strip.

### Validation rules

Packs are checked on import; a pack that fails is not installed and the reasons are listed.

- `id` must not be empty and must not contain `/`, `\` or `.`.
- There must be an `idle` action with art.
- Each strip: width = frames × `frame.w`, height = `frame.h`. All actions use the same frame size.
- Every referenced image and sound file must exist.
- `fallback` chains must not loop; `then` must point to an existing action.

### Installing a pack

- Gallery → **Import folder…** or **Import zip…**, or drag a folder or zip into the gallery window, or drop it onto any pet.
- If a pack with the same id exists, you see both version numbers and choose whether to overwrite. Overwriting a built-in pack does not delete the built-in version; deleting your copy restores it.
- Files in a zip may sit at the root or inside one folder.
- Packs live in `~/.familiar/pets/<id>/`. Copying a folder there works too: it loads within about 2 seconds, and edits to images or `pet.json` reload automatically.
- Imported packs have a **Delete** button (click twice to confirm). If the pack is in use by a team, project or rule, that slot falls back to the default pet.

### Art tips

- Draw the feet on the bottom row of the frame. Landing, standing on title bars and lining up for tug-of-war all use the bottom edge.
- About 100 px on screen works well: 32×32 pixel art with `scale: 3`, 64×64 with 2, 128×128 with 1.
- If both teams pick the same pet, the Codex one is hue-shifted 180° automatically; no need for a second color.
- Reference: the built-in [`pets/pixel-cat/`](pets/pixel-cat/), generated by [`tools/gen-builtin-pets.py`](tools/gen-builtin-pets.py).

### From a picture to a pet (optional)

The easiest way is in the app: gallery → **Make a pet from a picture…**. Pick or drop a picture (any background; it is removed automatically, on your computer), give it a name and optional lines, watch the preview of all 16 actions, and click Install. Step-by-step: [Make your own pet from one picture](docs/make-a-pet.md).

The same one-picture pack can be made from the command line with `python tools/petgen/build_pack.py <workdir> --image picture.png --id my-pet --name "My Pet"`.

For real frame-by-frame animation, [`tools/petgen/`](tools/petgen/README.md) also turns AI videos into a pack, using an image model and an image-to-video model:

1. Generate a chibi full-body base image on a solid green background from your picture (image model).
2. Use the base image as the first frame of one short video per action — idle, thinking, tool, attention, done, error, sleep (image-to-video model, static camera).
3. `build_pack.py` extracts frames with ffmpeg, keys out the green, picks loopable frame ranges, trims and packs everything into a standard pack with `pet.json`, and writes a contact sheet for checking.

Use any image and image-to-video models you like: produce `idle.mp4`, `thinking.mp4`, … in a work folder and run step 3. Prompts that work well are in [`tools/petgen/prompts.md`](tools/petgen/prompts.md); steps and tips in [`tools/petgen/README.md`](tools/petgen/README.md).

**Character rights:** a pack made from someone else's character is fine for personal use. Sharing or redistributing it needs the rights holder's permission.

## Build from source

Requirements: Rust 1.88+, Node 18+, WebView2 (included with Windows 10/11).

```bash
cd app && npm install && cd ..
node tools/build.mjs            # release build; add --debug for a faster build
```

This builds `target/release/familiar.exe` and copies `familiar-emit` into `adapters/claude-code/bin/`.

Installer: `node tools/build.mjs --bundle` produces `target/release/bundle/nsis/Familiar_<version>_x64-setup.exe`, which bundles the built-in pets, the Claude Code plugin and the emitter. Tauri downloads NSIS on the first bundle build.

Keep the npm `@tauri-apps/*` packages on the same minor version as the Rust Tauri crates (currently 2.11 / dialog 2.7), or `tauri build` refuses to bundle.

### Running from source

1. Run `familiar.exe` once by hand. It writes its path to `~/.familiar/petd_path`; from then on the emitter starts it automatically when it is not running.
2. Connect Claude Code with the repo as the marketplace:
   ```
   claude plugin marketplace add /path/to/familiar
   claude plugin install familiar@familiar
   ```
3. Connect Codex: `adapters\claude-code\bin\familiar-emit.exe --install codex`, then trust the hooks with `/hooks` in `codex`.

Claude Code copies the plugin into `~/.claude/plugins/cache/`. After a rebuild, bump the version in `adapters/claude-code/.claude-plugin/plugin.json`, update the plugin and restart Claude Code:

```
claude plugin marketplace update familiar
claude plugin update familiar@familiar
```

To change the built-in pets, edit `tools/gen-builtin-pets.py` and run `python tools/gen-builtin-pets.py` (needs Pillow).

Tests, debugging tips and contribution rules are in [CONTRIBUTING.md](CONTRIBUTING.md); design notes are in [docs/implementation-notes.md](docs/implementation-notes.md).

## Project layout

```
crates/emit/              hook emitter (Rust, single small executable) + Codex installer
crates/petd/              daemon core: state machine, routing, pet packs, config, HTTP/WS, world layer, SQLite
crates/petd/assets/       built-in balance.yaml (numbers), fish.yaml (fish), interactions.yaml (rules)
app/                      Tauri 2 shell + TypeScript/Canvas pages
  index.html              pet window (one per pet)
  card.html               hover session card
  gallery.html            gallery
  welcome.html            first-run picker
  overlay.html            overlay (one per monitor): bugs, tug rope, race bar
  report.html             daily report
adapters/claude-code/     Claude Code plugin: hooks/hooks.json, commands/pet.md, bin/
adapters/codex/           Codex event mapping (mapping.yaml)
pets/                     built-in packs (pixel-cat, pixel-bot)
tools/                    build script, built-in pet generator, petgen
docs/                     implementation notes
```

How it fits together: each agent hook runs `familiar-emit`, which posts a small event to the local daemon (`petd`, hosted inside the app) and exits within 300 ms, always with exit code 0 and no stdout, so it can never block or break your agent. If the app is not running, the event goes into a spool folder and the app is started in the background. The daemon keeps a state machine per session, routes sessions to pets and pushes state to the pet windows over a local WebSocket.

## Privacy and security

- **Local only.** The daemon listens on `127.0.0.1` (port 47321 by default, a random free port if taken) and every request needs the token in `~/.familiar/token`. Familiar makes no network requests and uploads nothing. All data stays in `~/.familiar/`.
- **Minimal event data.** Hooks pass the daemon only what the pets need: session id, event type, the session's working directory, tool name, edited file name and the agent's process id. Prompts, file contents and tool output are not forwarded. For Codex shell commands only the first word is used to tell reads from other tools; the command text never leaves the emitter.
- **Hooks are appended, never replaced.** The Claude Code side is a regular plugin. For Codex, Familiar appends its hook after yours and backs up `hooks.json` first; `--uninstall codex` removes it. Both "Hook up" actions tell you what they will change before doing it.
- **Codex quota is read-only.** The quota display reads Codex's own session logs (`$CODEX_HOME/sessions` and `~/.codex/sessions`) without modifying them. Familiar never uses a reset card on your behalf.
- **Local system information.** To detect closed sessions, jump to terminals and hide during fullscreen or screen sharing, the app reads the local process list and window titles. None of it is stored or sent anywhere.

## FAQ and troubleshooting

**Windows SmartScreen blocks the installer.**
The installer is not code-signed yet. Click **More info → Run anyway**, or build the installer yourself.

**The Codex pet never moves.**
Codex only runs trusted hooks. Run `codex` in a terminal, type `/hooks` and trust the Familiar hooks. If you use more than one Codex config folder, install into each one (see [Connect Codex](#3-connect-codex)).

**Claude Code still runs the old emitter after I rebuilt.**
Claude Code runs its cached copy of the plugin. Bump the plugin version, run `claude plugin marketplace update familiar` and `claude plugin update familiar@familiar` (or Tray → Hook up Claude Code… again, which updates an existing install), then restart Claude Code.

**The pets disappeared.**
Check in this order: the boss key (`Ctrl+Alt+H`); a fullscreen window in front; a meeting app's screen-sharing toolbar; `/pet hide` (undo with `/pet show` or Tray → Show pets).

**My meeting app is not detected while sharing.**
Add a word from its sharing toolbar's window title or class name to `meeting.share_windows` in `config.yaml`. Detection is based on those toolbars and is not guaranteed for every version.

**A pack does not show up.**
Open the gallery: packs that failed validation are listed there with the reason (the tray also shows a count).

**Double-click does not reach the right tab.**
Windows Terminal can be brought to the front, but not switched to a specific tab.

**Where are the logs?**
`~/.familiar/logs/petd.log`.

## Roadmap

- macOS support (not started).
- Code-signed installer.
- Dedicated catch/throw frames for ball toss (built-in pets reuse wave and cheer).
- A Codex pet-pack importer, once a real source format sample and licensing are confirmed.
- Broader screen-sharing detection.

## License

[MIT](LICENSE). The built-in pets (pixel cat and pixel bot) and their sounds are generated by code in this repository and are covered by the same license.

Third-party components shipped with the app: [onnxruntime-web](https://github.com/microsoft/onnxruntime) (MIT) and the [U-2-Net](https://github.com/xuebinqin/U-2-Net) u2netp model (Apache-2.0, see [`app/public/models/LICENSE-U2Net.txt`](app/public/models/LICENSE-U2Net.txt)), used for removing busy backgrounds in "Make a pet from a picture".

Packs you make from someone else's characters are for personal use; sharing them needs the rights holder's permission.
