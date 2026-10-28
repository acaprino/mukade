"""Live check: does Codex take back a session mukade wrote, with real encrypted reasoning?

    python -I live_codex.py <scratch dir>

Uses the user's real Codex login and store (`CODEX_HOME`, else ~/.codex): its tokens refresh
in place, so the store is not copied. Every thread it makes has `<scratch dir>/work` as its
working directory. Model calls, on the configured model at reasoning effort low, `--ignore-rules`
and `--ignore-user-config` off so the login is read:

- 1 makes the source (a question the model reasons about: an encrypted reasoning item), made once;
- Codex -> Codex: 1 resume of mukade's copy, 1 of the copy with one character of the encrypted
  payload changed (a negative control).
"""

import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
MUKADE = ROOT / "target" / "release" / ("mukade.exe" if sys.platform == "win32" else "mukade")
W = Path(sys.argv[1]).resolve()
WORK = W / "work"
WORK.mkdir(parents=True, exist_ok=True)
HOME = Path(os.environ.get("CODEX_HOME") or Path.home() / ".codex")
CODEX = os.environ.get("MUKADE_CODEX_EXE") or str(
    Path(os.environ.get("LOCALAPPDATA", "")) / "Programs/OpenAI/Codex/bin/codex.exe")
LOW = ["-c", 'model_reasoning_effort="low"', "--skip-git-repo-check", "--json"]
QUESTION = "What was your answer before? Reply with only the number."


def env():
    e = dict(os.environ, CODEX_HOME=str(HOME))
    # Behind a TLS-intercepting proxy whose CA Node programs get from NODE_EXTRA_CA_CERTS (as
    # in the harness this ran in), Codex, a Rust program, needs that CA named for itself.
    if e.get("NODE_EXTRA_CA_CERTS") and not e.get("CODEX_CA_CERTIFICATE"):
        e["CODEX_CA_CERTIFICATE"] = e["NODE_EXTRA_CA_CERTS"]
    return e


def codex(args, timeout=300):
    p = subprocess.run([CODEX, "exec", *args], cwd=WORK, env=env(), stdin=subprocess.DEVNULL,
                       capture_output=True, timeout=timeout, encoding="utf-8", errors="replace")
    events = []
    for line in p.stdout.splitlines():
        try:
            events.append(json.loads(line))
        except ValueError:
            pass
    thread = next((e.get("thread_id") for e in events if e.get("thread_id")), None)
    texts = [e["item"].get("text") for e in events
             if e.get("type") == "item.completed" and (e.get("item") or {}).get("type") == "agent_message"]
    errors = [e for e in events if e.get("type") in ("error", "turn.failed")]
    return {"exit": p.returncode, "thread": thread, "answer": texts[-1] if texts else None,
            "errors": json.dumps(errors)[-600:] if errors else None,
            "stderr": p.stderr.strip()[-400:]}


def rollout(thread):
    found = sorted((HOME / "sessions").rglob(f"rollout-*{thread}*.jsonl"))
    assert found, f"no rollout for {thread}"
    return found[-1]


def reasoning(path):
    out = []
    for line in Path(path).read_text(encoding="utf-8").splitlines():
        r = json.loads(line) if line.strip() else {}
        p = r.get("payload") or {}
        if r.get("type") == "response_item" and p.get("type") == "reasoning":
            out.append(p.get("encrypted_content"))
    return out


report = {}
SRC = W / "source.json"
if not SRC.exists():
    made = codex([*LOW, "What is 19 times 21? Think it through, then reply with only the number."])
    SRC.write_text(json.dumps(made), encoding="utf-8")
made = json.loads(SRC.read_text(encoding="utf-8"))
report["source_call"] = made
assert made["exit"] == 0 and made["answer"], f"the source call failed: {made}"
source = rollout(made["thread"])
report["source_reasoning"] = [len(x or "") for x in reasoning(source)]


def write(tag, edit=None):
    doc = W / f"{tag}.json"
    subprocess.run([str(MUKADE), "dump", "codex", made["thread"], "--root", str(HOME), "--output",
                    str(doc)], check=True, env=env())
    if edit:
        d = json.loads(doc.read_text(encoding="utf-8"))
        edit(d)
        doc.write_text(json.dumps(d), encoding="utf-8")
    p = subprocess.run([str(MUKADE), "write", "codex", str(doc), "--cwd", str(WORK),
                        "--codex-exe", CODEX], capture_output=True, text=True, env=env())
    result = {"write_exit": p.returncode, "stderr": p.stderr.strip()[-300:]}
    if p.returncode == 0:
        receipt = json.loads(p.stdout)
        m = receipt["manifest"]
        written = reasoning(receipt["location"])
        result.update(thread=receipt["id"], losses=m["losses"], strict=m["strict"],
                      written_reasoning=[len(x or "") for x in written],
                      payloads_identical=written == reasoning(source))
        result["resume"] = codex(["resume", *LOW, receipt["id"], QUESTION])
    report[tag] = result


def tamper(d):
    for m in d.get("messages", []):
        for b in m.get("content", []):
            o = b.get("opaque") if isinstance(b, dict) else None
            if o and o.get("encrypted"):
                v = o["encrypted"]
                i = len(v) // 2
                o["encrypted"] = v[:i] + ("A" if v[i] != "A" else "B") + v[i + 1:]
                return
    raise SystemExit("no encrypted payload to tamper with")


write("codex_to_codex")
write("codex_to_codex_tampered", tamper)
(W / "report-codex.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
print(json.dumps(report, indent=2))
