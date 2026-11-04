#!/usr/bin/env python3
"""Live acceptance of Mukade's hand-written native compaction records (spec §4.4).

    python -I live_native_compaction.py <scratch dir> [--destination claude|codex|opencode|pi]...
                                        [--no-resume] [--turns N] [--filler-chars N]

One synthetic Pi source is created locally, with no model call. It is deliberately far larger than
any destination model's context window. For each destination, `mukade hop --compaction native`
writes the whole history plus that destination's own native boundary; the real binary then resumes
it once and must answer from the deterministic summary. If a destination ignored the boundary, the
resume would have to send the whole oversized history and would fail or compact on its own.

Model calls: at most one per selected destination. No credential is printed or copied. Claude Code
and Codex use the user's real login; OpenCode and Pi use scratch homes and `OPENROUTER_API_KEY` from
the environment.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import shutil
import subprocess
import sys
import uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
MUKADE = str(ROOT / "target" / "release" / ("mukade.exe" if sys.platform == "win32" else "mukade"))
EARLY_CODE = "ALPHA-7391"
FINAL_REPLY = "DONE"
QUESTION = (
    "You are resuming a compacted session. What is the earliest secret code in the earlier "
    "conversation summary? Reply with only that code and nothing else."
)
NATIVE_BUDGET = ["--compaction", "native", "--context-window", "40000", "--max-output-tokens", "32000"]


class LiveError(RuntimeError):
    pass


def run(args, env=None, cwd=None, timeout=1200):
    p = subprocess.run(
        args,
        cwd=str(cwd) if cwd else None,
        env=env,
        stdin=subprocess.DEVNULL,
        capture_output=True,
        timeout=timeout,
        encoding="utf-8",
        errors="replace",
    )
    return {"exit": p.returncode, "stdout": p.stdout or "", "stderr": p.stderr or ""}


def short_id() -> str:
    return uuid.uuid4().hex[:8]


def iso(second: int) -> str:
    return f"2026-11-04T12:00:{second:02d}.000Z"


def millis(second: int) -> int:
    return int(dt.datetime(2026, 11, 4, 12, 0, second, tzinfo=dt.timezone.utc).timestamp() * 1000)


def filler(turn: int, chars: int) -> str:
    base = f"Filler turn {turn:05d}. The quick brown fox jumps over the lazy dog. "
    text = (base * ((chars // len(base)) + 1))[:chars]
    return f"{text} {turn:05d}"


def make_source(pi_home: Path, work: Path, turns: int, filler_chars: int) -> tuple[str, Path]:
    sid = str(uuid.uuid4())
    directory = pi_home / "sessions" / "--live-native-compaction--"
    directory.mkdir(parents=True, exist_ok=True)
    path = directory / f"2026-11-04T12-00-00-000Z_{sid}.jsonl"
    rows = [
        {
            "type": "session",
            "version": 3,
            "id": sid,
            "timestamp": iso(0),
            "cwd": str(work),
        },
        {
            "type": "model_change",
            "id": short_id(),
            "parentId": None,
            "timestamp": iso(0),
            "provider": "openrouter",
            "modelId": "anthropic/claude-haiku-5.5",
        },
    ]
    parent = rows[-1]["id"]
    second = 1

    def message(role: str, text: str, assistant: bool = False) -> None:
        nonlocal parent, second
        rid = short_id()
        content = [{"type": "text", "text": text}]
        msg = {"role": role, "content": content, "timestamp": millis(second)}
        if assistant:
            msg.update(
                {
                    "api": "anthropic-messages",
                    "provider": "openrouter",
                    "model": "anthropic/claude-haiku-5.5",
                    "usage": {
                        "input": 0,
                        "output": 0,
                        "cacheRead": 0,
                        "cacheWrite": 0,
                        "totalTokens": 0,
                        "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "total": 0},
                        "cacheWrite1h": 0,
                    },
                    "stopReason": "stop",
                    "responseId": f"msg_{rid}",
                }
            )
        rows.append(
            {
                "type": "message",
                "id": rid,
                "parentId": parent,
                "timestamp": iso(min(second, 59)),
                "message": msg,
            }
        )
        parent = rid
        second = (second + 1) % 60

    message("user", f"Secret sequence starts with {EARLY_CODE}. {filler(0, filler_chars)}")
    message("assistant", "ACK-00000", assistant=True)
    for turn in range(1, max(1, turns - 1)):
        message("user", filler(turn, filler_chars))
        message("assistant", f"ACK-{turn:05d}", assistant=True)
    message("user", "Final instruction: reply with only DONE.")
    message("assistant", FINAL_REPLY, assistant=True)
    path.write_text("".join(json.dumps(row, ensure_ascii=False) + "\n" for row in rows), encoding="utf-8")
    return sid, path


def rows_of(path: Path) -> list[dict]:
    return [json.loads(line) for line in Path(path).read_text(encoding="utf-8").splitlines() if line.strip()]


def text_of(value) -> str:
    return json.dumps(value, ensure_ascii=False)


def contains_early(value) -> bool:
    return EARLY_CODE in text_of(value)


def contains_final(value) -> bool:
    return FINAL_REPLY in text_of(value)


def mukade_hop(dest: str, sid: str, pi_home: Path, work: Path, env: dict, extra: list[str]) -> dict:
    harness = {"claude": "claude_code"}.get(dest, dest)
    manifest = work / f"manifest-{dest}.json"
    args = [
        MUKADE, "hop", "pi", sid, harness,
        "--root", str(pi_home),
        "--cwd", str(work),
        *NATIVE_BUDGET,
        "--reasoning", "drop",
        "--manifest", str(manifest),
        *extra,
    ]
    result = run(args, env=env, cwd=work, timeout=1800)
    if result["exit"] != 0:
        raise LiveError(f"mukade hop to {dest} failed: {result['stderr'][-1000:] or result['stdout'][-1000:]}")
    receipt = json.loads(result["stdout"].strip().splitlines()[-1])
    if receipt["manifest"]["options"]["compaction"] != "native":
        raise LiveError(f"{dest}: manifest does not record native compaction")
    return receipt


def answer_from_claude(result: dict) -> str:
    try:
        return str(json.loads(result["stdout"]).get("result", ""))
    except json.JSONDecodeError:
        return result["stdout"]


def answer_from_codex(result: dict) -> str:
    texts = []
    for line in result["stdout"].splitlines():
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue
        if event.get("type") == "item.completed" and (event.get("item") or {}).get("type") == "agent_message":
            texts.append(event["item"].get("text") or "")
    return texts[-1] if texts else result["stdout"]


def answer_from_opencode(result: dict) -> str:
    texts = []
    for line in result["stdout"].splitlines():
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue
        part = event.get("part") or {}
        if part.get("type") == "text" and part.get("text"):
            texts.append(part["text"])
    return texts[-1] if texts else result["stdout"]


def claude_destination(scratch: Path, work: Path, versions: dict):
    exe = shutil.which("claude")
    if not exe:
        raise LiveError("claude not found")
    config = Path.home() / ".claude"
    hop_env = dict(os.environ, CLAUDE_CONFIG_DIR=str(config))
    resume_env = {k: v for k, v in os.environ.items() if k != "CLAUDE_CONFIG_DIR"}
    extra = [
        "--dest-version", versions["claude"],
        "--dest-protocol", "anthropic-messages",
        "--model-provider", "anthropic",
        "--model", "claude-haiku-5-5",
        "--claude-exe", exe,
    ]

    def inspect(receipt: dict) -> dict:
        rows = rows_of(Path(receipt["location"]))
        boundaries = [i for i, r in enumerate(rows) if r.get("type") == "system" and r.get("subtype") == "compact_boundary"]
        if len(boundaries) != 1:
            raise LiveError(f"Claude: expected one compact_boundary, found {len(boundaries)}")
        at = boundaries[0]
        summary = rows[at + 1]
        preserved = rows[at]["compactMetadata"]["preservedMessages"]["uuids"]
        history = rows[:at]
        if summary.get("isCompactSummary") is not True:
            raise LiveError("Claude: the row after compact_boundary is not isCompactSummary")
        if not contains_early(summary):
            raise LiveError("Claude: the summary does not contain the early code")
        if not any(contains_early(r) for r in history):
            raise LiveError("Claude: the early history was dropped")
        if not any(contains_final(r) for r in history):
            raise LiveError("Claude: the final history was dropped")
        return {
            "native_records": len(boundaries),
            "history_rows": len(history),
            "preserved_uuids": len(preserved),
            "summary_has_early_code": True,
            "session": receipt["id"],
            "location": receipt["location"],
        }

    def resume(receipt: dict) -> dict:
        args = [
            exe, "-p", "--resume", receipt["id"], "--model", "haiku",
            "--setting-sources", "local", "--strict-mcp-config", "--output-format", "json", QUESTION,
        ]
        result = run(args, env=resume_env, cwd=work, timeout=600)
        return {"exit": result["exit"], "answer": answer_from_claude(result), "stderr": result["stderr"][-500:]}

    def cleanup(receipt: dict) -> dict:
        path = Path(receipt["location"])
        try:
            path.unlink(missing_ok=True)
            if path.parent.exists() and not any(path.parent.iterdir()):
                path.parent.rmdir()
            return {"removed": True}
        except OSError as error:
            return {"removed": False, "error": str(error)}

    return {
        "name": "claude",
        "version": versions["claude"],
        "hop_env": hop_env,
        "extra": extra,
        "inspect": inspect,
        "resume": resume,
        "cleanup": cleanup,
        "store": "user's real Claude Code configuration; only the scratch project folder is used",
    }


def codex_destination(scratch: Path, work: Path, versions: dict):
    exe = shutil.which("codex")
    if not exe:
        raise LiveError("codex not found")
    home = Path(os.environ.get("CODEX_HOME") or Path.home() / ".codex")
    env = dict(os.environ, CODEX_HOME=str(home))
    if env.get("NODE_EXTRA_CA_CERTS") and not env.get("CODEX_CA_CERTIFICATE"):
        env["CODEX_CA_CERTIFICATE"] = env["NODE_EXTRA_CA_CERTS"]
    extra = [
        "--dest-version", versions["codex"],
        "--dest-protocol", "openai-responses",
        "--model-provider", "openai",
        "--model", "gpt-6-astra",
        "--codex-exe", exe,
    ]

    def inspect(receipt: dict) -> dict:
        rows = rows_of(Path(receipt["location"]))
        compacted = [i for i, r in enumerate(rows) if r.get("type") == "compacted"]
        if len(compacted) != 1:
            raise LiveError(f"Codex: expected one compacted record, found {len(compacted)}")
        at = compacted[0]
        payload = rows[at]["payload"]
        history = rows[:at]
        replacement = payload.get("replacement_history") or []
        if not contains_early(payload.get("message")):
            raise LiveError("Codex: the compacted message does not contain the early code")
        if not any(contains_early(r.get("payload")) for r in history):
            raise LiveError("Codex: the early history was dropped")
        if not any(contains_final(r.get("payload")) for r in history):
            raise LiveError("Codex: the final history was dropped")
        if len(replacement) < 2 or not contains_early(replacement[0]):
            raise LiveError("Codex: replacement_history does not carry the summary first")
        if not contains_final(replacement[-1]):
            raise LiveError("Codex: replacement_history does not retain the final reply")
        if any(item.get("id") is not None for item in replacement):
            raise LiveError("Codex: replacement_history must not carry user-supplied item IDs")
        return {
            "native_records": len(compacted),
            "history_rows": len(history),
            "replacement_history_items": len(replacement),
            "summary_has_early_code": True,
            "thread": receipt["id"],
            "location": receipt["location"],
        }

    def resume(receipt: dict) -> dict:
        args = [
            exe, "exec", "resume", receipt["id"],
            "-c", 'model_reasoning_effort="low"',
            "-c", 'sandbox_mode="read-only"',
            "-m", "gpt-6-astra",
            "--skip-git-repo-check",
            "--json",
            QUESTION,
        ]
        result = run(args, env=env, cwd=work, timeout=600)
        return {"exit": result["exit"], "answer": answer_from_codex(result), "stderr": result["stderr"][-500:]}

    def cleanup(receipt: dict) -> dict:
        result = run([exe, "delete", "--force", receipt["id"]], env=env, cwd=work, timeout=120)
        return {"removed": result["exit"] == 0, "exit": result["exit"], "stderr": result["stderr"][-300:]}

    return {
        "name": "codex",
        "version": versions["codex"],
        "hop_env": env,
        "extra": extra,
        "inspect": inspect,
        "resume": resume,
        "cleanup": cleanup,
        "store": "user's real CODEX_HOME; the throwaway thread is deleted with codex delete --force",
    }


def opencode_destination(scratch: Path, work: Path, versions: dict):
    exe = shutil.which("opencode.cmd") or shutil.which("opencode")
    if not exe:
        raise LiveError("opencode not found")
    if not os.environ.get("OPENROUTER_API_KEY"):
        raise LiveError("OPENROUTER_API_KEY must be set")
    xdg = scratch / "opencode-xdg"
    home = scratch / "opencode-home"
    for name in ("data", "config", "state", "cache"):
        (xdg / name).mkdir(parents=True, exist_ok=True)
    home.mkdir(parents=True, exist_ok=True)
    env = {k: v for k, v in os.environ.items() if k not in {"ANTHROPIC_API_KEY", "OPENAI_API_KEY", "OPENCODE_CONFIG", "MUKADE_OPENCODE_EXE"}}
    env.update(
        OPENCODE_DB=str(xdg / "data" / "opencode.db"),
        XDG_DATA_HOME=str(xdg / "data"),
        XDG_CONFIG_HOME=str(xdg / "config"),
        XDG_STATE_HOME=str(xdg / "state"),
        XDG_CACHE_HOME=str(xdg / "cache"),
        HOME=str(home),
        USERPROFILE=str(home),
    )
    model = "openrouter/anthropic/claude-haiku-5.5"
    extra = [
        "--dest-version", versions["opencode"],
        "--dest-protocol", "anthropic-messages",
        "--model-provider", "openrouter",
        "--model", "anthropic/claude-haiku-5.5",
        "--project-id", "global",
        "--opencode-exe", exe,
    ]

    def export(session: str) -> dict:
        result = run([exe, "session", "export", session, "--standalone"], env=env, cwd=work, timeout=300)
        if result["exit"] != 0:
            raise LiveError(f"OpenCode export failed: {result['stderr'][-500:]}")
        return json.loads(result["stdout"])

    def inspect(receipt: dict) -> dict:
        document = export(receipt["id"])
        messages = document["messages"]
        compactions = [i for i, m in enumerate(messages) if m.get("type") == "compaction"]
        if len(compactions) != 1:
            raise LiveError(f"OpenCode: expected one compaction, found {len(compactions)}")
        at = compactions[0]
        boundary = messages[at]
        history = messages[:at]
        if boundary.get("status") != "completed" or boundary.get("native") is not None:
            raise LiveError("OpenCode: the boundary is not a completed textual compaction")
        if boundary.get("metadata", {}).get("keepID") != history[-1]["id"]:
            raise LiveError("OpenCode: keepID does not point back at the retained tail")
        if not contains_early(boundary.get("summary")):
            raise LiveError("OpenCode: the summary does not contain the early code")
        if not any(contains_early(m) for m in history):
            raise LiveError("OpenCode: the early history was dropped")
        if not any(contains_final(m) for m in history):
            raise LiveError("OpenCode: the final history was dropped")
        return {
            "native_records": len(compactions),
            "history_messages": len(history),
            "keep_id": boundary["metadata"]["keepID"],
            "summary_has_early_code": True,
            "session": receipt["id"],
        }

    def resume(receipt: dict) -> dict:
        args = [exe, "run", "--standalone", "--session", receipt["id"], "-m", model, "--format", "json", QUESTION]
        result = run(args, env=env, cwd=work, timeout=600)
        return {"exit": result["exit"], "answer": answer_from_opencode(result), "stderr": result["stderr"][-500:]}

    def cleanup(receipt: dict) -> dict:
        result = run([exe, "session", "delete", receipt["id"], "--standalone"], env=env, cwd=work, timeout=120)
        return {"removed": result["exit"] == 0, "exit": result["exit"], "stderr": result["stderr"][-300:]}

    return {
        "name": "opencode",
        "version": versions["opencode"],
        "hop_env": env,
        "extra": extra,
        "inspect": inspect,
        "resume": resume,
        "cleanup": cleanup,
        "store": "scratch OpenCode database and XDG folders",
    }


def pi_destination(scratch: Path, work: Path, versions: dict):
    exe = shutil.which("pi.cmd") or shutil.which("pi")
    if not exe:
        raise LiveError("pi not found")
    if not os.environ.get("OPENROUTER_API_KEY"):
        raise LiveError("OPENROUTER_API_KEY must be set")
    home = scratch / "pi-home"
    profile = scratch / "pi-profile"
    home.mkdir(parents=True, exist_ok=True)
    profile.mkdir(parents=True, exist_ok=True)
    env = {k: v for k, v in os.environ.items() if k not in {"ANTHROPIC_API_KEY", "OPENAI_API_KEY", "GEMINI_API_KEY", "CLAUDE_CONFIG_DIR", "CODEX_HOME", "OPENCODE_DB", "MUKADE_PI_EXE"}}
    env.update(
        PI_CODING_AGENT_DIR=str(home),
        HOME=str(profile),
        USERPROFILE=str(profile),
        PI_SKIP_VERSION_CHECK="1",
        PI_TELEMETRY="0",
    )
    extra = [
        "--dest-version", versions["pi"],
        "--dest-protocol", "anthropic-messages",
        "--model-provider", "openrouter",
        "--model", "anthropic/claude-haiku-5.5",
        "--pi-exe", exe,
    ]
    quiet = [
        "--no-extensions", "--no-mcp", "--no-skills", "--no-prompt-templates",
        "--no-themes", "--no-context-files", "--no-tools",
    ]

    def inspect(receipt: dict) -> dict:
        rows = rows_of(Path(receipt["location"]))
        compactions = [i for i, r in enumerate(rows) if r.get("type") == "compaction"]
        if len(compactions) != 1:
            raise LiveError(f"Pi: expected one compaction entry, found {len(compactions)}")
        at = compactions[0]
        boundary = rows[at]
        history = rows[:at]
        if boundary.get("firstKeptEntryId") != history[-1]["id"]:
            raise LiveError("Pi: firstKeptEntryId does not point back at the retained tail")
        if not contains_early(boundary.get("summary")):
            raise LiveError("Pi: the summary does not contain the early code")
        if not any(contains_early(r) for r in history):
            raise LiveError("Pi: the early history was dropped")
        if not any(contains_final(r) for r in history):
            raise LiveError("Pi: the final history was dropped")
        return {
            "native_records": len(compactions),
            "history_rows": len(history),
            "first_kept_entry_id": boundary["firstKeptEntryId"],
            "summary_has_early_code": True,
            "session": receipt["id"],
            "location": receipt["location"],
        }

    def resume(receipt: dict) -> dict:
        args = [
            exe, "-p", "--session", receipt["location"],
            "--provider", "openrouter", "--model", "anthropic/claude-haiku-5.5",
            *quiet, QUESTION,
        ]
        result = run(args, env=env, cwd=work, timeout=600)
        return {"exit": result["exit"], "answer": result["stdout"].strip(), "stderr": result["stderr"][-500:]}

    def cleanup(receipt: dict) -> dict:
        path = Path(receipt["location"])
        try:
            path.unlink(missing_ok=True)
            return {"removed": True}
        except OSError as error:
            return {"removed": False, "error": str(error)}

    return {
        "name": "pi",
        "version": versions["pi"],
        "hop_env": env,
        "extra": extra,
        "inspect": inspect,
        "resume": resume,
        "cleanup": cleanup,
        "store": "scratch Pi home",
    }


DESTINATIONS = {
    "claude": claude_destination,
    "codex": codex_destination,
    "opencode": opencode_destination,
    "pi": pi_destination,
}


def versions() -> dict:
    claude = run([shutil.which("claude"), "--version"], timeout=60)["stdout"].strip().split()[0]
    codex = run([shutil.which("codex"), "--version"], timeout=60)["stdout"].strip().split()[-1]
    opencode = run([shutil.which("opencode.cmd") or shutil.which("opencode"), "--version"], timeout=60)["stdout"].strip().split()[-1].lstrip("v")
    pi = run([shutil.which("pi.cmd") or shutil.which("pi"), "--version"], timeout=60)["stdout"].strip().splitlines()[0]
    return {"claude": claude, "codex": codex, "opencode": opencode, "pi": pi}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("scratch", type=Path)
    parser.add_argument("--destination", action="append", choices=sorted(DESTINATIONS), dest="destinations")
    parser.add_argument("--no-resume", action="store_true")
    parser.add_argument("--turns", type=int, default=2000)
    parser.add_argument("--filler-chars", type=int, default=3000)
    args = parser.parse_args()

    scratch = args.scratch.resolve()
    scratch.mkdir(parents=True, exist_ok=True)
    work = scratch / "work"
    work.mkdir(exist_ok=True)
    pi_home = scratch / "source-pi-home"
    pi_home.mkdir(exist_ok=True)
    selected = args.destinations or list(DESTINATIONS)
    report = {"mukade": str(MUKADE), "versions": versions(), "destinations": {}}

    source_file = scratch / "source-path.txt"
    if source_file.exists():
        sid = source_file.read_text(encoding="utf-8").splitlines()[0]
        source = Path(source_file.read_text(encoding="utf-8").splitlines()[1])
    else:
        sid, source = make_source(pi_home, work, args.turns, args.filler_chars)
        source_file.write_text(f"{sid}\n{source}\n", encoding="utf-8")
    report["source"] = {
        "id": sid,
        "path": str(source),
        "bytes": source.stat().st_size,
        "turns": args.turns,
        "filler_chars": args.filler_chars,
    }

    for name in selected:
        entry: dict = {"selected": True}
        report["destinations"][name] = entry
        try:
            dest = DESTINATIONS[name](scratch, work, report["versions"])
            entry.update({"version": dest["version"], "store": dest["store"]})
            receipt = mukade_hop(name, sid, pi_home, work, dest["hop_env"], dest["extra"])
            entry["manifest"] = {
                "compaction": receipt["manifest"]["options"]["compaction"],
                "losses": receipt["manifest"]["losses"],
                "strict": receipt["manifest"]["strict"],
            }
            entry["written"] = dest["inspect"](receipt)
            if not args.no_resume:
                resumed = dest["resume"](receipt)
                entry["resume"] = resumed
                entry["resume"]["answered_with_early_code"] = EARLY_CODE in resumed.get("answer", "")
                entry["after_resume"] = dest["inspect"](receipt)
                entry["no_extra_native_compaction"] = entry["after_resume"]["native_records"] == 1
                entry["accepted"] = bool(
                    resumed["exit"] == 0
                    and entry["resume"]["answered_with_early_code"]
                    and entry["no_extra_native_compaction"]
                )
            entry["cleanup"] = dest["cleanup"](receipt)
        except Exception as error:  # noqa: BLE001 - one destination must not stop the others
            entry["error"] = f"{type(error).__name__}: {error}"
            entry["accepted"] = False
    report_path = scratch / "report-native-compaction.json"
    report_path.write_text(json.dumps(report, indent=2, ensure_ascii=False), encoding="utf-8")
    print(json.dumps(report, indent=2, ensure_ascii=False))
    return 0 if all(entry.get("accepted") or args.no_resume for entry in report["destinations"].values()) else 1


if __name__ == "__main__":
    raise SystemExit(main())
