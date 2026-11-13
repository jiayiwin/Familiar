// Build Rill and stage the emitter inside the Claude Code plugin.
//
//   node tools/build.mjs           release build
//   node tools/build.mjs --debug   faster debug build
//   node tools/build.mjs --bundle  release build + NSIS installer
import { execSync } from "node:child_process";
import { copyFileSync, mkdirSync, renameSync } from "node:fs";
import { join, dirname } from "node:path";
import { homedir } from "node:os";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const debug = process.argv.includes("--debug");
const bundle = process.argv.includes("--bundle");

// Release binaries would otherwise carry the builder's home and checkout paths
// (panic locations, C asserts). Later remaps win, so the broad ones go first.
const env = { ...process.env };
if (!debug) {
  const remaps = [
    [homedir(), "~"],
    [process.env.RUSTUP_HOME || join(homedir(), ".rustup"), "rustup"],
    [process.env.CARGO_HOME || join(homedir(), ".cargo"), "cargo"],
    [root, "rillbuddy"],
  ];
  const inherited = (process.env.CARGO_ENCODED_RUSTFLAGS ?? "").split("\x1f").concat((process.env.RUSTFLAGS ?? "").split(/\s+/)).filter(Boolean);
  env.CARGO_ENCODED_RUSTFLAGS = [...inherited, ...remaps.map(([from, to]) => `--remap-path-prefix=${from}=${to}`)].join("\x1f");
  delete env.RUSTFLAGS;
  if (process.platform === "win32") {
    // MSVC's switch for trimming __FILE__ in the C dependencies.
    env.CL = [process.env.CL, ...remaps.filter(([from]) => !/\s/.test(from)).map(([from]) => `/d1trimfile:${from}\\`)].filter(Boolean).join(" ");
  }
}
const run = (cmd, cwd = root) => execSync(cmd, { cwd, env, stdio: "inherit" });

run("npm run build", join(root, "app"));
// custom-protocol embeds app/dist; without it the exe tries to load the Vite dev server.
run(`cargo build ${debug ? "" : "--release"} -p rillbuddy -p rillbuddy-emit --features rillbuddy/custom-protocol`);

const exe = process.platform === "win32" ? ".exe" : "";
const out = join(root, "target", debug ? "debug" : "release");
const bin = join(root, "adapters", "claude-code", "bin");
mkdirSync(bin, { recursive: true });
const staged = join(bin, `rillbuddy-emit${exe}`);
try {
  copyFileSync(join(out, `rillbuddy-emit${exe}`), staged);
} catch (e) {
  if (e.code !== "EBUSY" && e.code !== "EPERM") throw e;
  // A hook somewhere is running the staged emitter right now. A running
  // program can be moved but not overwritten: park the old one under target/
  // (outside the folder the installer ships) and put the new one in its place.
  renameSync(staged, join(out, `rillbuddy-emit.in-use-${Date.now()}${exe}`));
  copyFileSync(join(out, `rillbuddy-emit${exe}`), staged);
  console.log("the staged emitter was in use: moved it aside");
}
console.log(`\nemitter -> ${bin}\napp     -> ${join(out, `rillbuddy${exe}`)}`);

if (bundle) {
  // The installer ships adapters/claude-code (with the emitter staged above).
  run("npx tauri build --bundles nsis --features custom-protocol", join(root, "app"));
  console.log(`installer -> ${join(root, "target", "release", "bundle", "nsis")}`);
}
