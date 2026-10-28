"""Live check: does Claude Code take back sessions mukade wrote, with real signed and redacted
thinking?

    python -I live_claude.py <scratch dir> [<pi session file with redacted thinking>]

Uses the user's real Claude Code login and store, with Claude Code's default configuration
(CLAUDE_CONFIG_DIR unset: set, even to its default folder, Claude Code makes a new configuration
file inside it). Every session it makes is filed under the project folder of `<scratch dir>/work`,
apart from the user's projects. Model calls, all Claude Haiku at effort high:

- 1 makes the source (a question Haiku thinks about: signed thinking), made once;
- Claude -> Claude: 1 resume of mukade's copy, 1 of the copy with one character of the signature
  changed (a negative control: it must be refused);
- Pi -> Claude, when a Pi session holding Anthropic redacted thinking is given (made by
  ../pi-reasoning-live with Anthropic's trigger): 1 resume.
"""

import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
MUKADE = ROOT / "target" / "release" / ("mukade.exe" if sys.platform == "win32" else "mukade")
W = Path(sys.argv[1]).resolve()
PI_REDACTED = Path(sys.argv[2]).resolve() if len(sys.argv) > 2 else None
WORK = W / "work"
WORK.mkdir(parents=True, exist_ok=True)
CLAUDE = shutil.which("claude")
assert "CLAUDE_CONFIG_DIR" not in os.environ, "run with Claude Code's default configuration"
CONFIG = Path.home() / ".claude"
# No user or project settings (so no hooks or plugins), no MCP servers; Haiku thinks at effort
# high, as in the user's own sessions.
QUIET = ["--model", "haiku", "--effort", "high", "--setting-sources", "local",
         "--strict-mcp-config", "--output-format", "json"]
QUESTION = "What were your answers so far? Reply with only the numbers."


def mukade_env():
    """mukade's environment: the store named explicitly."""
    return dict(os.environ, CLAUDE_CONFIG_DIR=str(CONFIG))


def claude(args, timeout=300):
    p = subprocess.run([CLAUDE, "-p", *args], cwd=WORK, env=dict(os.environ),
                       stdin=subprocess.DEVNULL, capture_output=True, timeout=timeout,
                       encoding="utf-8", errors="replace")
    try:
        out = json.loads(p.stdout)
    except ValueError:
        out = {"raw": p.stdout.strip()[-400:]}
    return {"exit": p.returncode, "result": str(out.get("result", out.get("raw", "")))[-300:],
            "is_error": out.get("is_error"), "session_id": out.get("session_id"),
            "stderr": p.stderr.strip()[-500:]}


def session_file(sid):
    found = list((CONFIG / "projects").glob(f"*/{sid}.jsonl"))
    assert found, f"no session file for {sid}"
    return found[0]


def thinking(path):
    out = []
    for line in Path(path).read_text(encoding="utf-8").splitlines():
        r = json.loads(line) if line.strip() else {}
        if r.get("type") != "assistant":
            continue
        for b in (r.get("message") or {}).get("content") or []:
            if isinstance(b, dict) and b.get("type") in ("thinking", "redacted_thinking"):
                out.append(b)
    return out


def describe(bs):
    return [{"type": b["type"], "signature_len": len(b.get("signature") or ""),
             "data_len": len(b.get("data") or "")} for b in bs]


def payloads(bs):
    return [b.get("signature") or b.get("data") for b in bs]


report = {}
SRC = W / "source.json"
if not SRC.exists():
    made = claude([*QUIET, "What is 19 times 21? Think it through, then reply with only the number."])
    SRC.write_text(json.dumps(made), encoding="utf-8")
made = json.loads(SRC.read_text(encoding="utf-8"))
report["source_call"] = made
sid = made["session_id"]
source = session_file(sid)
report["source_thinking"] = describe(thinking(source))


def write(source_harness, source_id, root, tag, edit=None):
    """mukade dump -> (an edit of the document) -> write into Claude Code -> resume once."""
    doc = W / f"{tag}.json"
    subprocess.run([str(MUKADE), "dump", source_harness, source_id, "--root", str(root),
                    "--output", str(doc)], check=True, env=mukade_env())
    if edit:
        d = json.loads(doc.read_text(encoding="utf-8"))
        edit(d)
        doc.write_text(json.dumps(d), encoding="utf-8")
    p = subprocess.run([str(MUKADE), "write", "claude_code", str(doc), "--cwd", str(WORK),
                        "--dest-protocol", "anthropic-messages"],
                       capture_output=True, text=True, env=mukade_env())
    result = {"write_exit": p.returncode, "stderr": p.stderr.strip()[-300:]}
    if p.returncode == 0:
        receipt = json.loads(p.stdout)
        m = receipt["manifest"]
        written = thinking(receipt["location"])
        result.update(session=receipt["id"], losses=m["losses"], strict=m["strict"],
                      written_thinking=describe(written), written_payloads=payloads(written))
        result["resume"] = claude([*QUIET, "--resume", receipt["id"], QUESTION])
    report[tag] = result
    return result


def tamper(kind):
    def edit(d):
        for m in d.get("messages", []):
            for b in m.get("content", []):
                o = b.get("opaque") if isinstance(b, dict) else None
                if o and o.get(kind):
                    v = o[kind]
                    i = len(v) // 2
                    o[kind] = v[:i] + ("A" if v[i] != "A" else "B") + v[i + 1:]
                    return
        raise SystemExit(f"no {kind} to tamper with")
    return edit


r = write("claude_code", sid, CONFIG, "claude_to_claude")
r["payloads_identical"] = r.get("written_payloads") == payloads(thinking(source))
write("claude_code", sid, CONFIG, "claude_to_claude_tampered", tamper("signature"))

if PI_REDACTED:
    pi_home = next(p for p in PI_REDACTED.parents if p.name == "sessions").parent
    pi_id = PI_REDACTED.stem.split("_", 1)[1]
    write("pi", pi_id, pi_home, "pi_redacted_to_claude")

for tag in [k for k in report if isinstance(report[k], dict)]:
    report[tag].pop("written_payloads", None)
(W / "report-claude.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
print(json.dumps(report, indent=2))
