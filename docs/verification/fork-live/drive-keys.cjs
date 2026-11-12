// Drive a full-screen terminal program in a pseudo-console and save what its screen shows after
// each step, rendered by a headless xterm. Usage:
//   node drive-keys.cjs <node-pty dir> <@xterm/headless dir> <steps.json> <out dir>
// steps.json: {"exe": "...", "args": [...], "cwd": "...", "env": {"NAME": "value" | null},
//              "cols": 160, "rows": 50,
//              "steps": [{"wait": 3000}, {"keys": "e", "wait": 2000, "snap": "after-e"}, ...]}
// A key string may hold "<enter>", "<esc>", "<tab>", "<up>", "<down>", "<left>", "<right>",
// "<bs>"; a step with "until": "<regex>" waits (up to its "wait") for the screen to match.
const [ptyDir, xtermDir, stepsFile, outDir] = process.argv.slice(2);
const pty = require(ptyDir);
const { Terminal } = require(xtermDir);
const fs = require("fs");
const path = require("path");
const plan = JSON.parse(fs.readFileSync(stepsFile, "utf8"));
const cols = plan.cols ?? 160;
const rows = plan.rows ?? 50;
const env = { ...process.env };
// Markers a parent Claude Code passes down; with them a child Claude Code saves no transcript.
for (const k of Object.keys(env)) if (/^CLAUDE(CODE|_CODE_|_PID|_EFFORT)/.test(k)) delete env[k];
for (const [k, v] of Object.entries(plan.env ?? {})) {
  if (v === null) delete env[k];
  else env[k] = v;
}
const term = new Terminal({ cols, rows, allowProposedApi: true, scrollback: 20000 });
const child = pty.spawn(plan.exe, plan.args ?? [], { name: "xterm-256color", cols, rows, cwd: plan.cwd, env, useConpty: true });
child.onData((d) => term.write(d));
let exited = null;
child.onExit((e) => { exited = e; });
const KEYS = { enter: "\r", esc: "\x1b", tab: "\t", up: "\x1b[A", down: "\x1b[B", right: "\x1b[C", left: "\x1b[D", bs: "\x7f" };
const encode = (s) => s.replace(/<(enter|esc|tab|up|down|left|right|bs)>/g, (_, k) => KEYS[k]);
// The visible screen; with `full`, the scrollback above it too (a program that prints its
// history inline, as Codex does).
const screen = (full = false) => {
  const b = term.buffer.active;
  const lines = [];
  const [from, to] = full ? [0, b.length] : [b.viewportY, b.viewportY + rows];
  for (let i = from; i < to; i++) lines.push((b.getLine(i)?.translateToString(true) ?? ""));
  return lines.join("\n").replace(/\n+$/, "") + "\n";
};
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
(async () => {
  fs.mkdirSync(outDir, { recursive: true });
  let n = 0;
  for (const step of plan.steps) {
    n++;
    if (step.keys) {
      // One key at a time: a TUI reads each as its own event. An escape sequence (arrows,
      // PageUp `ESC [5~`) is one key: split, it reads as Escape and then typed text.
      for (const part of encode(step.keys).match(/\x1b\[[0-9;]*[A-Za-z~]|[\s\S]/g) ?? []) {
        child.write(part);
        await sleep(step.gap ?? 60);
      }
    }
    if (step.answers) {
      // Questions a program asks on its way (a folder to trust, hooks to review), in any order:
      // each answer is typed once, when its question shows, until `until` shows with no question
      // left on screen for `settle` ms.
      const re = new RegExp(step.until ?? "$^", "i");
      const done = new Set();
      const end = Date.now() + (step.wait ?? 60000);
      let calm = 0;
      while (Date.now() < end) {
        const s = screen();
        const open = step.answers.findIndex((a, i) => !done.has(i) && new RegExp(a.when, "i").test(s));
        if (open >= 0) {
          done.add(open);
          fs.appendFileSync(path.join(outDir, "log.txt"), `step ${n} answered /${step.answers[open].when}/\n`);
          for (const part of encode(step.answers[open].keys).match(/\x1b\[[0-9;]*[A-Za-z~]|[\s\S]/g) ?? []) {
            child.write(part);
            await sleep(200);
          }
          calm = 0;
          await sleep(1500);
          continue;
        }
        const asking = step.answers.some((a) => new RegExp(a.when, "i").test(s));
        calm = re.test(s) && !asking ? calm + 250 : 0;
        if (calm >= (step.settle ?? 4000)) break;
        await sleep(250);
      }
      fs.appendFileSync(path.join(outDir, "log.txt"), `step ${n} until /${step.until}/: ${re.test(screen())}\n`);
    } else if (step.until) {
      const re = new RegExp(step.until, "i");
      const end = Date.now() + (step.wait ?? 10000);
      while (Date.now() < end && !re.test(screen())) await sleep(250);
      fs.appendFileSync(path.join(outDir, "log.txt"), `step ${n} until /${step.until}/: ${re.test(screen())}\n`);
    } else {
      await sleep(step.wait ?? 500);
    }
    if (step.snap) fs.writeFileSync(path.join(outDir, `${String(n).padStart(2, "0")}-${step.snap}.txt`), screen(step.full));
    if (exited) break;
  }
  fs.writeFileSync(path.join(outDir, "final.txt"), screen(true));
  fs.appendFileSync(path.join(outDir, "log.txt"), `exited: ${JSON.stringify(exited)}\n`);
  try { child.kill(); } catch {}
  await sleep(500);
  process.exit(0);
})();
