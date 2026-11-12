// Drive an interactive Claude Code in a pseudo-console: resume a session, type `/branch <name>`,
// then `/exit`. No prompt goes to the model. Usage: node drive-branch.cjs <node-pty dir> <cwd> <session id> <name> <log>
const [ptyDir, cwd, sessionId, name, logPath] = process.argv.slice(2);
const pty = require(ptyDir);
// The Claude Code executable: CLAUDE_EXE, else where the native installer puts it.
const path = require("path");
const CLAUDE = process.env.CLAUDE_EXE || path.join(process.env.USERPROFILE, ".local", "bin", "claude.exe");
const fs = require("fs");
const env = { ...process.env };
// Markers a parent Claude Code passes down; with them the child saves no transcript.
for (const k of Object.keys(env)) if (/^CLAUDE(CODE|_CODE_|_PID|_EFFORT)/.test(k)) delete env[k];
const term = pty.spawn(CLAUDE, ["--resume", sessionId], {
  name: "xterm-256color", cols: 140, rows: 45, cwd, env, useConpty: true,
});
let raw = "";
const plain = () => raw.replace(/\x1b\[[0-9;?]*[ -\/]*[@-~]/g, "").replace(/\x1b\][^\x07]*\x07/g, "").replace(/\x1b[()][0-9A-Za-z]/g, "");
term.onData((d) => { raw += d; });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
async function waitFor(re, ms) {
  const end = Date.now() + ms;
  while (Date.now() < end) { if (re.test(plain())) return true; await sleep(250); }
  return false;
}
(async () => {
  const log = (m) => fs.appendFileSync(logPath, m + "\n");
  try {
    // Either a trust question or the input box comes first.
    const ready = /shift\+tab|for\s*shortcuts|Do\s*you\s*trust|trust\s*the\s*files|Yes,\s*proceed|Yes,\s*I\s*trust/i;
    if (!(await waitFor(ready, 60000))) { log("TIMEOUT waiting for start"); return; }
    if (/Do\s*you\s*trust|trust\s*the\s*files|Yes,\s*proceed|Yes,\s*I\s*trust/i.test(plain()) && !/shift\+tab/i.test(plain())) {
      log("trust dialog: accepting");
      term.write("\r");
      await sleep(3000);
    }
    if (!(await waitFor(/shift\+tab|for\s*shortcuts/i, 60000))) { log("TIMEOUT waiting for input box"); return; }
    await sleep(2500);
    const before = plain().length;
    term.write(`/branch ${name}`);
    await sleep(1500);
    term.write("\r");
    const branched = await waitFor(/Branched\s*conversation/i, 30000);
    log("branched: " + branched);
    const tail = plain().slice(before);
    const m = tail.match(/Branched[\s\S]*?([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})/i);
    log("new session: " + (m ? m[1] : "not printed"));
    await sleep(2000);
    term.write("/exit");
    await sleep(1200);
    term.write("\r");
    await waitFor(/\bGoodbye|Resume this session|claude --resume/i, 15000);
  } finally {
    fs.writeFileSync(logPath + ".screen.txt", plain());
    await sleep(1000);
    try { term.kill(); } catch {}
    process.exit(0);
  }
})();
