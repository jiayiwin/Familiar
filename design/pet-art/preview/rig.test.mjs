// Runs every mode of the rig through a full round on a music clock that starts
// below zero (as the audio clock does), drawing each frame on a stub canvas.
//   node design/pet-art/preview/rig.test.mjs
import { readFileSync, copyFileSync, mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const copy = join(mkdtempSync(join(tmpdir(), "rig-")), "rig.mjs");
copyFileSync(join(here, "rig.js"), copy);
const { RigPet, drawKeyboard, drawCello, danceMoveAt } = await import(pathToFileURL(copy).href);

const load = (who) => {
  const rig = JSON.parse(readFileSync(join(here, `../${who}/rig/rig.json`), "utf-8"));
  rig.back = JSON.parse(readFileSync(join(here, `../${who}/rig/back.json`), "utf-8"));
  rig.faces = JSON.parse(readFileSync(join(here, `../${who}/rig/faces.json`), "utf-8"));
  return rig;
};
const gradient = { addColorStop() {} };
const ctx = new Proxy({}, { get: (_, k) => (k === "createRadialGradient" || k === "createLinearGradient" ? () => gradient : () => {}), set: () => true });

let frames = 0;
for (const [who, play] of [["zimeng", "piano"], ["mashu", "cello"], ["paopao", "flute"], ["gugu", "xylophone"], ["rongrong", "harp"]]) {
  const rig = load(who);
  const moves = new Set();
  for (const [mode, beat] of [["idle", 0], [play, 0.84], ["dance", 0.5]]) {
    for (const expr of mode === "idle" ? ["neutral", "happy", "sleepy", "asleep", "worried", "surprised"] : ["neutral"]) {
      const pet = new RigPet(rig, {});
      pet.expr = expr;
      pet.setMode(mode);
      for (let i = 0; i < 40; i++) pet.tap(i % 2 ? "l" : "r", 0.4 + i * 0.42);
      for (let i = 0; i < 30; i++) pet.bow(0.4 + i * 1.3, i % 3 ? 0.84 : 1.68);
      for (let i = 0; i < 60; i++) pet.note(0.4 + i * 0.6, i % 4 ? 0.4 : 1.6, 50 + ((i * 7) % 24));
      const reactions = ["poke", "perk", "cheer"];
      for (let t = -0.15; t < 40; t += 1 / 60) {
        const dt = frames % 97 === 0 ? 0.3 : 1 / 60; // an occasional dropped frame
        const music = beat ? { time: t, beats: t / beat } : null;
        if (frames % 150 === 0) pet.react(reactions[(frames / 150) % 3]);
        // The kegel pose, on and off, while idling.
        if (mode === "idle" && expr === "neutral") pet.setBack(t > 12 && t < 30, t > 14 ? 0.5 + 0.5 * Math.sin(t * 2) : 0);
        // Played with from outside while it idles: chased about, spun, made to look and to smile.
        if (mode === "idle" && expr === "happy") pet.play = { x: 60 * Math.sin(t), hop: 10 * Math.abs(Math.sin(t * 5)), rot: 14 * Math.sin(t * 0.7), twirl: t * 3, yaw: Math.sin(t * 2), pitch: Math.cos(t), head: 8 * Math.sin(t), look: [3 * Math.sin(t), -2], face: t % 4 < 2 ? "surprised" : null };
        pet.update(dt, music);
        pet.draw(ctx, 1, mode === "piano" ? (c) => drawKeyboard(c, [{ white: 3, black: false, glow: 1 }], [{ x: 1, y: 1, life: 0.5 }], 1) : mode === "cello" ? (c) => drawCello(c, 150, 450, 220, 1, 0.5) : null);
        if (mode === "dance") moves.add(danceMoveAt(music.beats, pet.kind));
        const nums = [...Object.values(pet.root), pet.ears.l.angle, pet.ears.r.angle, pet.ears.l.stretch, pet.open, pet.tail.x, pet.head.x, pet.bowTie.x, pet.bowPos, pet.top.x, pet.topY.x, pet.pulse, pet.propA, pet.reach, ...pet.glints.flatMap((g) => [g.x, g.y, g.life, g.tip]), ...pet.bars.filter((g) => g !== undefined), ...pet.mallets.flatMap((m) => [m.at, m.hit]), ...pet.glow.filter((g) => g !== undefined), ...pet.bubbles.flatMap((b) => [b.x, b.y, b.life])];
        if (!nums.every(Number.isFinite)) throw new Error(`${who} ${mode}/${expr} @ ${t.toFixed(2)}s: non-finite pose ${JSON.stringify(nums)}`);
        if (pet.kind === "lop" && (pet.ears.l.angle < -9.01 || pet.ears.r.angle > 9.01)) throw new Error(`${who} ${mode} @ ${t.toFixed(2)}s: an ear swung over an eye`);
        if (Math.abs(pet.bowPos) > 1.001) throw new Error(`${who} ${mode} @ ${t.toFixed(2)}s: the bow left the strings`);
        if (Math.abs(pet.top.x) > 25.01 || (pet.kind === "mushroom" && Math.abs(pet.top.x) > 8.01)) throw new Error(`${who} ${mode} @ ${t.toFixed(2)}s: what rides on its head swung too far (${pet.top.x})`);
        // The ear on the harp's side reaches into the strings while it plays; otherwise both stay near their roots.
        const far = mode === "harp" ? 46.01 : 18.01;
        if (pet.kind === "sheep" && (pet.ears.l.angle > far || pet.ears.l.angle < -18.01 || Math.abs(pet.ears.r.angle) > 18.01)) throw new Error(`${who} ${mode} @ ${t.toFixed(2)}s: an ear swung past its root`);
        if (pet.bubbles.length > 40) throw new Error(`${who} ${mode}: bubbles pile up`);
        frames++;
      }
    }
  }
  if (moves.size < 6) throw new Error(`${who}: a dance round should use at least 6 distinct moves, saw ${[...moves]}`);
  // Each of the five dances its own round: no tail swish for a pet with no tail in front, no ear flick without ears that stand.
  const own = { zimeng: "earWave", mashu: "tailSwish", paopao: "jelly", gugu: "capTip", rongrong: "earSwing" }[who];
  if (!moves.has(own)) throw new Error(`${who}: its round should have ${own}, saw ${[...moves]}`);
  if (["paopao", "gugu"].includes(who) && (moves.has("tailSwish") || moves.has("earFlick"))) throw new Error(`${who} dances with parts it does not have: ${[...moves]}`);
  console.log(`ok ${who}: dance moves: ${[...moves].join(", ")}`);
}
console.log(`ok: ${frames} frames`);
