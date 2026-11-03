# petgen: a picture → a pet pack

[中文](#中文)

Turn one character picture into a Familiar pet pack. Two ways:

- **One picture, no AI** (`--image`): the character is cut out and every action is animated from it: squash, tilt, hop, plus small drawn effects. Takes a second. Most people should use the gallery's **Make a pet from a picture…** button, which does the same inside the app ([tutorial](../../docs/make-a-pet.md)); this is the command-line version.
  ```bash
  python tools/petgen/build_pack.py work/xiaoling --image tools/petgen/examples/xiaoling.jpg --id xiaoling --name 小灵 --name-en Ling
  ```
- **AI videos** (below): make a chibi base image, animate each action with an image-to-video model, then extract frames, key out the green and pack everything up.

Generation is **bring your own models**: any image model that takes a reference picture and any image-to-video model that takes a first frame will do. This folder only ships the generic part, [`build_pack.py`](build_pack.py), plus the prompts that work well ([`prompts.md`](prompts.md)). Nothing here runs inside the app; the result is a standard pet pack you import from the gallery.

## Requirements

- Python 3 with Pillow and numpy
- ffmpeg on PATH (video route only)

## Steps

```bash
mkdir -p work/my-pet
```

1. **Base image.** Feed your character picture to an image model with the base prompt from [`prompts.md`](prompts.md). You want a full-body chibi, facing the viewer, on a **solid pure green (#00FF00) background**. Generate two or three and pick the best.
2. **Action videos.** For each action (idle, thinking, tool, attention, done, error, sleep), run an image-to-video model with the base image as the **first frame** and the action prompt + the shared suffix from `prompts.md`. Short clips (about 4 s) at low resolution (480p) are plenty. Save them as `work/my-pet/idle.mp4`, `thinking.mp4`, … Only `idle` is required; missing actions fall back to idle.
3. **Pack.**
   ```bash
   python tools/petgen/build_pack.py work/my-pet --id my-pet --name 我的宠物 --name-en "My Pet" \
     --tags "character,AI" --bubbles work/my-pet/bubbles.json --bubbles-en work/my-pet/bubbles_en.json
   ```
   Output: `work/my-pet/pack/my-pet/` (drag it into the gallery) and `work/my-pet/contact.png`, every action on a checkerboard so you can check the keying.

Speech bubbles are optional: `{ "attention": ["Your turn!"], "done": ["Done"], "error": ["Oops"], "stone": ["Give me a sec"] }`.

## What gets built

| Action | Source | Frames |
|---|---|---|
| idle, thinking, tool, attention, error | one clip each; the most loopable stretch is picked automatically | 4–6 |
| done | the whole clip, plays once, then back to idle | 8 |
| sleep | second half of the clip (already seated and asleep) | 4 |
| wake | the "sitting down" frames of the sleep clip, reversed | 4 |
| stone, aborted | not generated; borrow error (stone is tinted grey) | — |
| fishing, tug-of-war and other game actions | not generated; fall back to idle, games still work | — |

Sounds reuse the built-in pets' chimes, which are synthesized in code.

## Tips

- The base image **must** have a flat green background: video models need an opaque first frame, and keying relies on it. Avoid large bright-green areas on the character.
- Every action prompt asks for a static camera, the whole body in frame and the character staying in place. For jumps, ask for a small jump that stays inside the frame, or the head gets cut off.
- Keying lives in `key_green()` in `build_pack.py`; tune its two thresholds if green spills onto the character.
- Rights: a pack made from someone else's character is fine for personal use. Sharing it needs the rights holder's permission.

---

## 中文

把一张角色图做成 Familiar 宠物包，两种方式：

- **一张图、不用 AI**（`--image`）：自动抠出人物，用压扁、歪头、蹦跳加小特效做出全部动作，一秒完成。普通用户直接用图鉴里的 **用图片做宠物…** 按钮就行，效果一样（[教程](../../docs/make-a-pet.zh-CN.md)）；这里是命令行版本。
- **AI 视频**（下文）：先生成 Q 版立绘，再用图生视频做出每个动作，最后自动抽帧、抠绿幕、打包。

生成环节**用你自己的模型**：能参考图片出图的图片模型、能以首帧生成视频的图生视频模型都可以。这个目录只放通用的部分：[`build_pack.py`](build_pack.py) 和好用的提示词（[`prompts.md`](prompts.md)）。这些都不在 App 里运行，产物是标准宠物包，用图鉴导入。

**依赖**：Python 3（Pillow、numpy）；视频路线还需要命令行能直接运行 ffmpeg。

**步骤**

1. **立绘**：把角色图和 `prompts.md` 里的立绘提示词交给图片模型，要全身 Q 版、正面、**纯绿色（#00FF00）背景**。多出两三张挑一张。
2. **动作视频**：每个动作（idle、thinking、tool、attention、done、error、sleep）用立绘作**首帧**，提示词 = 动作提示词 + 通用后缀，跑一段图生视频。4 秒左右、480p 就够。存成 `work/my-pet/idle.mp4`、`thinking.mp4`……只有 idle 是必需的，缺的动作会借用 idle。
3. **打包**：运行上面英文部分的 `build_pack.py` 命令。`--name-en`、`--bubbles-en` 可选，写进 pet.json 的 `name_en`、`bubbles_en`，界面切到英文时使用。产物是 `work/my-pet/pack/my-pet/`（拖进图鉴）和 `contact.png`（所有动作铺在棋盘格上，用来检查抠像）。

**经验**

- 立绘必须是纯绿色背景：视频模型需要不透明的首帧，抠像也靠它。人物身上不要有大面积鲜绿色。
- 动作提示词都要求镜头固定、全身在画面里、人物不离开原位。跳跃类动作要强调"跳得低一点、不出画"。
- 抠像在 `build_pack.py` 的 `key_green()` 里，绿色溢出时调那两个阈值。
- 版权：用别人的角色自己用没问题；分享需要版权方授权。
