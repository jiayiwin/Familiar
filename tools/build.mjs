// Build Familiar and stage the emitter inside the Claude Code plugin.
//
//   node tools/build.mjs           release build
//   node tools/build.mjs --debug   faster debug build
//   node tools/build.mjs --bundle  release build + NSIS installer
import { execSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const debug = process.argv.includes("--debug");
const bundle = process.argv.includes("--bundle");
const run = (cmd, cwd = root) => execSync(cmd, { cwd, stdio: "inherit" });

run("npm run build", join(root, "app"));
// custom-protocol embeds app/dist; without it the exe tries to load the Vite dev server.
run(`cargo build ${debug ? "" : "--release"} -p familiar -p familiar-emit --features familiar/custom-protocol`);

const exe = process.platform === "win32" ? ".exe" : "";
const out = join(root, "target", debug ? "debug" : "release");
const bin = join(root, "adapters", "claude-code", "bin");
mkdirSync(bin, { recursive: true });
copyFileSync(join(out, `familiar-emit${exe}`), join(bin, `familiar-emit${exe}`));
console.log(`\nemitter -> ${bin}\napp     -> ${join(out, `familiar${exe}`)}`);

if (bundle) {
  // The installer ships adapters/claude-code (with the emitter staged above).
  run("npx tauri build --bundles nsis --features custom-protocol", join(root, "app"));
  console.log(`installer -> ${join(root, "target", "release", "bundle", "nsis")}`);
}
