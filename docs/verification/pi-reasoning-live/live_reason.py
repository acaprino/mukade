"""Live check: does a real model's signed reasoning survive a mukade hop into Pi?

    python -I live_reason.py <scratch dir>

Pi runs in a scratch home (see live_env.py), through OpenRouter, with Claude Haiku 5.5 at thinking
level `low`. Four model calls: one makes the source session, then one resume for each of three hops
Pi -> Pi (reasoning dropped by default / kept natively with --dest-protocol / written as text). The
source session is made once; a rerun reuses it.
"""

import json
import shutil
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from live_env import env as live_env  # noqa: E402

ROOT = Path(__file__).resolve().parents[3]
MUKADE = ROOT / "target" / "release" / ("mukade.exe" if sys.platform == "win32" else "mukade")
W = Path(sys.argv[1]).resolve()
MODEL = ["--provider", "openrouter", "--model", "anthropic/claude-haiku-5.5", "--thinking", "low"]
QUIET = ["--no-extensions", "--no-mcp", "--no-skills", "--no-prompt-templates", "--no-themes",
         "--no-context-files", "--no-tools"]
PI = shutil.which("pi.cmd") or shutil.which("pi")
LABEL = "[earlier reasoning, not replayable]"

(W / "pi-home").mkdir(parents=True, exist_ok=True)
(W / "home").mkdir(exist_ok=True)
WORK = W / "work"
WORK.mkdir(exist_ok=True)


def env():
    return live_env(W)


def run(args, timeout=180):
    p = subprocess.run(args, cwd=WORK, env=env(), stdin=subprocess.DEVNULL, capture_output=True,
                       timeout=timeout, encoding="utf-8", errors="replace")
    return p.returncode, p.stdout, p.stderr


def rows(path):
    return [json.loads(line) for line in Path(path).read_text(encoding="utf-8").splitlines()
            if line.strip()]


def blocks_of(msg):
    content = msg.get("content")
    if isinstance(content, str):
        return [{"type": "text", "text": content}]
    return [b for b in content or [] if isinstance(b, dict)]


def thinking_blocks(path):
    out = []
    for r in rows(path):
        msg = r.get("message") or {}
        if r.get("type") == "message" and msg.get("role") == "assistant":
            for block in blocks_of(msg):
                if block.get("type") == "thinking":
                    out.append({"api": msg.get("api"), "provider": msg.get("provider"),
                                "model": msg.get("model"),
                                "text_len": len(block.get("thinking") or ""),
                                "signature": block.get("thinkingSignature")})
    return out


def texts(path):
    return [block.get("text") for r in rows(path) if r.get("type") == "message"
            for block in blocks_of(r.get("message") or {}) if block.get("type") == "text"]


report = {"pi_version": run([PI, "--version"])[1].strip()}

# 1. A real source session with reasoning (made once).
SRC = W / "source-session.txt"
if not SRC.exists():
    code, out, err = run([PI, "-p", *MODEL, *QUIET,
                          "What is 17 times 23? Work it out, then reply with only the number."])
    (W / "source-call.json").write_text(json.dumps(
        {"exit": code, "stdout": out.strip()[-200:], "stderr": err.strip()[-400:]}),
        encoding="utf-8")
    sessions = sorted((W / "pi-home" / "sessions").rglob("*.jsonl"))
    assert sessions, f"no source session: {err}"
    SRC.write_text(str(sessions[0]), encoding="utf-8")
report["source_call"] = json.loads((W / "source-call.json").read_text(encoding="utf-8"))
source = Path(SRC.read_text(encoding="utf-8"))
source_signatures = [b["signature"] for b in thinking_blocks(source)]
report["source_thinking"] = [{**b, "signature": (b["signature"] or "")[:16] + "…",
                              "signature_len": len(b["signature"] or "")}
                             for b in thinking_blocks(source)]

# 2. What mukade reads from it.
sid = source.stem.split("_", 1)[1]
doc = W / "doc.json"
subprocess.run([str(MUKADE), "dump", "pi", sid, "--root", str(W / "pi-home"), "--output",
                str(doc)], check=True, env=env())
document = json.loads(doc.read_text(encoding="utf-8"))
opaque = [b.get("opaque") for m in document.get("messages", []) for b in m.get("content", [])
          if isinstance(b, dict) and b.get("type") == "thinking"]
report["dump_opaque"] = [{"protocol": (o or {}).get("protocol"),
                          "signature_len": len((o or {}).get("signature") or ""),
                          "encrypted_len": len((o or {}).get("encrypted") or "")} for o in opaque]
protocol = (opaque[0] or {}).get("protocol") if opaque else None
report["source_protocol"] = protocol


# 3. Three hops Pi -> Pi, each resumed once with a real call.
def hop(tag, extra):
    p = subprocess.run([str(MUKADE), "hop", "pi", sid, "pi", "--root", str(W / "pi-home"),
                        "--cwd", str(WORK), *extra], capture_output=True, text=True, env=env())
    result = {"args": extra, "exit": p.returncode, "stderr": p.stderr.strip()[-300:]}
    if p.stdout.strip().startswith("{"):
        receipt = json.loads(p.stdout)
        m = receipt["manifest"]
        written = Path(receipt["location"])
        blocks = thinking_blocks(written)
        result.update(
            session=written.name,
            reasoning_dropped=m["counts"]["reasoning_dropped"], losses=m["losses"],
            strict=m["strict"], written_thinking=len(blocks),
            signature_identical=([b["signature"] for b in blocks] == source_signatures
                                 if blocks else None),
            labelled_text=any(LABEL in (t or "") for t in texts(written)))
        code, out, err = run([PI, "-p", "--session", str(written), *MODEL, *QUIET,
                              "What number did you answer before? Reply with only that number."])
        result["resume"] = {"exit": code, "stdout": out.strip()[-200:],
                            "stderr": err.strip()[-500:]}
    report[tag] = result


hop("hop_default", [])
hop("hop_native", ["--dest-protocol", protocol] if isinstance(protocol, str) else [])
hop("hop_as_text", ["--reasoning", "as_text"])

(W / "report.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
print(json.dumps(report, indent=2))
