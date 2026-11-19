// mukade chat: the part every plugin kit shares. The launcher puts the mukade executable and the
// harness name above this line as `const MUKADE`; the room, the role and the service's credential
// come from the environment the launcher set, never from this file.
import { spawn } from "node:child_process";

/** One hook call: `mukade chat hook <harness> <event>`, the event's JSON on standard input. The
 *  answer is one JSON object (`{context}` or `{continue}`), or null when there is nothing to hand
 *  over. A failure is never the harness's problem: it answers null. `signal` stops the call. */
function mukadeHook(event, input, signal) {
  return new Promise((resolve) => {
    let child;
    try {
      child = spawn(MUKADE.exe, ["chat", "hook", MUKADE.harness, event], {
        stdio: ["pipe", "pipe", "ignore"],
        windowsHide: true,
      });
    } catch {
      resolve(null);
      return;
    }
    let out = "";
    let settled = false;
    const done = (value) => {
      if (settled) return;
      settled = true;
      signal?.removeEventListener("abort", stop);
      resolve(value);
    };
    const stop = () => {
      try {
        child.kill();
      } catch {}
      done(null);
    };
    if (signal?.aborted) return stop();
    signal?.addEventListener("abort", stop, { once: true });
    child.stdout.setEncoding("utf8");
    child.stdout.on("data", (chunk) => {
      out += chunk;
    });
    child.on("error", () => done(null));
    child.on("close", () => {
      const line = out.trim().split("\n").pop();
      if (!line) return done(null);
      try {
        done(JSON.parse(line));
      } catch {
        done(null);
      }
    });
    child.stdin.on("error", () => {});
    child.stdin.end(JSON.stringify(input));
  });
}
