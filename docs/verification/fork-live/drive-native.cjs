// An interactive Claude Code in a pseudo-console: one typed prompt, then `/branch <name>`, then
// `/exit`. Usage: node drive-native.cjs <node-pty dir> <cwd> <prompt> <branch name> <log>
const [ptyDir, cwd, prompt, name, logPath] = process.argv.slice(2);
const pty = require(ptyDir);
// The Claude Code executable: CLAUDE_EXE, else where the native installer puts it.
const path = require("path");
const CLAUDE = process.env.CLAUDE_EXE || path.join(process.env.USERPROFILE, ".local", "bin", "claude.exe");
const fs = require("fs");
const env = { ...process.env };
// Markers a parent Claude Code passes down; with them the child saves no transcript.
for (const k of Object.keys(env)) if (/^CLAUDE(CODE|_CODE_|_PID|_EFFORT)/.test(k)) delete env[k];
const term = pty.spawn(CLAUDE, ["--model", "haiku"], {
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
async function waitFor(test, ms) {
  const end = Date.now() + ms;
  while (Date.now() < end) { if (test()) return true; await sleep(300); }
  return false;
}
const log = (m) => fs.appendFileSync(logPath, m + "\n");
(async () => {
  try {
    if (!(await waitFor(() => /trust/i.test(plain()) || /shift\+tab/i.test(plain()), 60000))) { log("TIMEOUT start"); return; }
    if (/Do\s*you\s*trust|trust\s*the\s*files|Yes,\s*proceed/i.test(plain()) && !/shift\+tab/i.test(plain())) {
      log("trust dialog: accepting"); term.write("\r"); await sleep(3000);
    }
    if (!(await waitFor(() => /shift\+tab/i.test(plain()), 60000))) { log("TIMEOUT input"); return; }
    await sleep(2500);
    term.write(prompt);
    await sleep(800);
    term.write("\r");
    // The answer: the word the prompt asks for, after the prompt itself.
    const mark = raw.length;
    const answered = await waitFor(() => {
      const t = raw.slice(mark);
      return /ORIGIN/.test(t.replace(/Reply with only the word ORIGIN/g, "")) && !/esc\s*to\s*interrupt/i.test(t.slice(-400));
    }, 120000);
    log("answered: " + answered);
    await sleep(3000);
    const before = raw.length;
    term.write(`/branch ${name}`);
    await sleep(1500);
    term.write("\r");
    const branched = await waitFor(() => /Branched\s*conversation/i.test(plain()), 30000);
    log("branched: " + branched);
    const m = plain().match(/Branched[\s\S]*?([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})/i);
    log("new session: " + (m ? m[1] : "not printed"));
    await sleep(2000);
    term.write("/exit");
    await sleep(1200);
    term.write("\r");
    await sleep(4000);
  } finally {
    fs.writeFileSync(logPath + ".screen.txt", plain());
    try { term.kill(); } catch {}
    process.exit(0);
  }
})();
