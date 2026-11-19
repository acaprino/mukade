"""The environment both scripts run Pi in: a scratch home, and OpenRouter's key from the caller's
environment only (no credential file is read or copied)."""

import os


def env(w):
    e = dict(os.environ)
    for key in ("ANTHROPIC_API_KEY", "OPENAI_API_KEY", "GEMINI_API_KEY", "CLAUDE_CONFIG_DIR",
                "CODEX_HOME", "OPENCODE_DB", "MUKADE_PI_EXE"):
        e.pop(key, None)
    assert e.get("OPENROUTER_API_KEY"), "OPENROUTER_API_KEY must be set"
    e.update(PI_CODING_AGENT_DIR=str(w / "pi-home"), HOME=str(w / "home"),
             USERPROFILE=str(w / "home"), PI_SKIP_VERSION_CHECK="1", PI_TELEMETRY="0")
    return e
