"""Live check: does OpenCode take back sessions mukade wrote, with real signed and redacted
Anthropic thinking?

    python -I live_opencode.py <scratch dir> <pi session: signed thinking> [<pi session: redacted>]

OpenCode runs with a scratch database and scratch XDG folders, through OpenRouter with Claude
Haiku 5.5 (`OPENROUTER_API_KEY` from the environment; no credential file is read or copied). The
sources are Pi sessions made by ../pi-reasoning-live (signed thinking) and with Anthropic's
redacted-thinking trigger. Model calls: 1 resume of each copy, and 1 of the signed copy with one
character of its signature changed (a negative control).
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
SOURCES = [Path(p).resolve() for p in sys.argv[2:]]
WORK = W / "work"
WORK.mkdir(parents=True, exist_ok=True)
OPENCODE = shutil.which("opencode.cmd") or shutil.which("opencode")
MODEL = "openrouter/anthropic/claude-haiku-5.5"
QUESTION = "What number did you answer before? Reply with only that number."


def env():
    e = dict(os.environ)
    for key in ("ANTHROPIC_API_KEY", "OPENAI_API_KEY", "OPENCODE_CONFIG", "MUKADE_OPENCODE_EXE"):
        e.pop(key, None)
    assert e.get("OPENROUTER_API_KEY"), "OPENROUTER_API_KEY must be set"
    for name in ("data", "config", "state", "cache"):
        (W / "xdg" / name).mkdir(parents=True, exist_ok=True)
    e.update(OPENCODE_DB=str(W / "xdg" / "data" / "opencode.db"),
             XDG_DATA_HOME=str(W / "xdg" / "data"), XDG_CONFIG_HOME=str(W / "xdg" / "config"),
             XDG_STATE_HOME=str(W / "xdg" / "state"), XDG_CACHE_HOME=str(W / "xdg" / "cache"),
             HOME=str(W / "home"), USERPROFILE=str(W / "home"))
    (W / "home").mkdir(exist_ok=True)
    return e


def resume(session):
    p = subprocess.run([OPENCODE, "run", "--standalone", "--session", session, "-m", MODEL,
                        "--format", "json", QUESTION], cwd=WORK, env=env(),
                       stdin=subprocess.DEVNULL, capture_output=True, timeout=300,
                       encoding="utf-8", errors="replace")
    texts, errors = [], []
    for line in p.stdout.splitlines():
        try:
            e = json.loads(line)
        except ValueError:
            continue
        part = e.get("part") or {}
        if part.get("type") == "text" and part.get("text"):
            texts.append(part["text"])
        if e.get("type") == "error" or e.get("error"):
            errors.append(e)
    return {"exit": p.returncode, "answer": texts[-1] if texts else None,
            "errors": json.dumps(errors)[-600:] if errors else None,
            "stderr": p.stderr.strip()[-600:]}


def write(source, tag, edit=None):
    home = next(p for p in source.parents if p.name == "sessions").parent
    sid = source.stem.split("_", 1)[1]
    doc = W / f"{tag}.json"
    subprocess.run([str(MUKADE), "dump", "pi", sid, "--root", str(home), "--output", str(doc)],
                   check=True, env=env())
    if edit:
        d = json.loads(doc.read_text(encoding="utf-8"))
        edit(d)
        doc.write_text(json.dumps(d), encoding="utf-8")
    p = subprocess.run([str(MUKADE), "write", "opencode", str(doc), "--cwd", str(WORK),
                        "--opencode-exe", OPENCODE, "--project-id", "global",
                        "--model-provider", "openrouter", "--model", "anthropic/claude-haiku-5.5",
                        "--dest-protocol", "anthropic-messages"],
                       capture_output=True, text=True, env=env())
    result = {"write_exit": p.returncode, "stderr": p.stderr.strip()[-400:]}
    if p.returncode == 0:
        receipt = json.loads(p.stdout)
        m = receipt["manifest"]
        result.update(session=receipt["id"], losses=m["losses"], strict=m["strict"])
        result["resume"] = resume(receipt["id"])
    report[tag] = result


def tamper(d):
    for m in d.get("messages", []):
        for b in m.get("content", []):
            o = b.get("opaque") if isinstance(b, dict) else None
            if o and o.get("signature"):
                v = o["signature"]
                i = len(v) // 2
                o["signature"] = v[:i] + ("A" if v[i] != "A" else "B") + v[i + 1:]
                return
    raise SystemExit("no signature to tamper with")


report = {"opencode": subprocess.run([OPENCODE, "--version"], capture_output=True, text=True,
                                     env=env()).stdout.strip()}
write(SOURCES[0], "pi_signed_to_opencode")
write(SOURCES[0], "pi_signed_to_opencode_tampered", tamper)
if len(SOURCES) > 1:
    write(SOURCES[1], "pi_redacted_to_opencode")
(W / "report-opencode.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
print(json.dumps(report, indent=2))
