# petgen prompts

Prompts that work well with common image and image-to-video models. Use them as-is or as a starting point.

## Base image (image model, with your character picture as reference)

> Turn the character in the reference image into a cute chibi desktop-pet mascot. Full body, standing straight, facing the viewer, arms relaxed at the sides, feet visible at the bottom. Chibi proportions about 2.5 heads tall, big head, small body. Keep the character's identity: hair, eye color, face, outfit, colors and accessories. Clean cel-shaded cartoon style, bold dark outline, flat colors, game sprite look. Centered with empty margin around. Background: one solid flat pure green color (#00FF00), no gradient, no floor, no shadow, no other objects, no text.

Add a sentence describing the character (hair, outfit, props) when the reference only shows the head, or to steer details, e.g. "Dress her modestly: long skirt, covered shoulders."

Square (1:1) at about 1K is enough.

## Action videos (image-to-video, base image as the first frame)

Append this suffix to every action prompt:

> Static camera, fixed framing, the whole body always fully visible and centered, the character stays in place and never leaves the frame. Keep the solid flat pure green background exactly unchanged, no shadow, no props on the ground. Same chibi cartoon style, outline and colors as the first frame. Smooth, gentle, loopable motion.

| File | Prompt |
|---|---|
| `idle.mp4` | The chibi character stands idle: gentle breathing, blinks twice, hair sways slightly, tiny relaxed body bob. |
| `thinking.mp4` | The chibi character puts one hand on their chin and looks up thoughtfully, tilting their head slightly, as if pondering a hard problem. |
| `tool.mp4` | The chibi character conjures a small glowing magic scroll floating in front of their chest and writes on it quickly with their fingers, focused and busy. |
| `attention.mp4` | The chibi character raises one hand high and waves eagerly at the viewer, surprised and urgent expression, as if calling for attention. |
| `done.mp4` | The chibi character jumps a little with both fists in the air, big smile, staying fully inside the frame, then lands back on the same spot. |
| `error.mp4` | The chibi character slumps their shoulders, looks down sadly and scratches their head, embarrassed and frustrated, a small sweat drop. |
| `sleep.mp4` | The chibi character sits down on the ground cross-legged, closes their eyes and dozes off, head slowly nodding, peaceful sleep. |

About 4 seconds at 480p per clip, no audio.
