"""Negative control for live_reason.py, run after it on the same scratch dir:

    python -I live_reason_control.py <scratch dir>

It copies the session the native hop wrote, changes one character in the middle of the signature
of the thinking block the hop carried, and resumes the copy once. If Pi really sends the block back
and the provider checks it, the resume must be refused; if it succeeds, the native hop's successful
resume proved nothing about the signature.
"""

import json
import shutil
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from live_env import env  # noqa: E402

W = Path(sys.argv[1]).resolve()
PI = shutil.which("pi.cmd") or shutil.which("pi")
TAMPERED_ID = "00000000-0000-4000-8000-00000000c0de"

report = json.loads((W / "report.json").read_text(encoding="utf-8"))
source = Path((W / "source-session.txt").read_text(encoding="utf-8"))
native = source.parent / report["hop_native"]["session"]

rows = [json.loads(line) for line in native.read_text(encoding="utf-8").splitlines()
        if line.strip()]
changed = 0
for r in rows:
    for block in (r.get("message") or {}).get("content") or []:
        # Only the block the hop carried (the first); later ones are the resume's own.
        is_signed = (isinstance(block, dict) and block.get("type") == "thinking"
                     and block.get("thinkingSignature"))
        if changed == 0 and is_signed:
            sig = block["thinkingSignature"]
            i = len(sig) // 2
            block["thinkingSignature"] = sig[:i] + ("A" if sig[i] != "A" else "B") + sig[i + 1:]
            changed += 1
assert changed == 1, changed
if rows[0].get("type") == "session":
    rows[0]["id"] = TAMPERED_ID
tampered = native.with_name(native.name.split("_", 1)[0] + "_" + TAMPERED_ID + ".jsonl")
tampered.write_text("".join(json.dumps(r) + "\n" for r in rows), encoding="utf-8")

p = subprocess.run([PI, "-p", "--session", str(tampered), "--provider", "openrouter", "--model",
                    "anthropic/claude-haiku-5.5", "--thinking", "low", "--no-extensions",
                    "--no-mcp", "--no-skills", "--no-prompt-templates", "--no-themes",
                    "--no-context-files", "--no-tools",
                    "What number did you answer before? Reply with only that number."],
                   cwd=W / "work", env=env(W), stdin=subprocess.DEVNULL, capture_output=True,
                   timeout=180, encoding="utf-8", errors="replace")
result = {"native_session": native.name, "tampered_session": tampered.name,
          "exit": p.returncode, "stdout": p.stdout.strip()[-300:],
          "stderr": p.stderr.strip()[-600:]}
(W / "control.json").write_text(json.dumps(result, indent=2), encoding="utf-8")
print(json.dumps(result, indent=2))
