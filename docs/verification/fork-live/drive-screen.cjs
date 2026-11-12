// Start Claude Code in a pseudo-console with the given arguments, wait, save the screen, quit.
// Nothing is typed but Escape and /exit. Usage:
//   node drive-screen.cjs <node-pty dir> <cwd> <out file> <wait ms> [claude args...]
const [ptyDir, cwd, out, waitMs, ...args] = process.argv.slice(2);
const pty = require(ptyDir);
// The Claude Code executable: CLAUDE_EXE, else where the native installer puts it.
const path = require("path");
const CLAUDE = process.env.CLAUDE_EXE || path.join(process.env.USERPROFILE, ".local", "bin", "claude.exe");
const fs = require("fs");
const env = { ...process.env };
// Markers a parent Claude Code passes down; with them the child saves no transcript.
for (const k of Object.keys(env)) if (/^CLAUDE(CODE|_CODE_|_PID|_EFFORT)/.test(k)) delete env[k];
const term = pty.spawn(CLAUDE, args, {
  name: "xterm-256color", cols: 160, rows: 50, cwd, env, useConpty: true,
});
let raw = "";
term.onData((d) => { raw += d; });
const plain = () => raw
  .replace(/\x1b\[(\d*)C/g, (_, n) => " ".repeat(Number(n || 1)))
  .replace(/\x1b\[\d+;\d+H/g, "\n")
  .replace(/\x1b\[[0-9;?>]*[ -\/]*[@-~]/g, "")
  .replace(/\x1b\][^\x07]*\x07/g, "")
  .replace(/\x1b[()][0-9A-Za-z]/g, "")
  .replace(/\r/g, "");
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
(async () => {
  await sleep(Number(waitMs));
  fs.writeFileSync(out, plain());
  term.write("\x1b");
  await sleep(800);
  term.write("\x1b");
  await sleep(800);
  term.write("/exit");
  await sleep(800);
  term.write("\r");
  await sleep(2500);
  try { term.kill(); } catch {}
  process.exit(0);
})();
