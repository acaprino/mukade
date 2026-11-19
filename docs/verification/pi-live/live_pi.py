"""Live conformance run of mukade against a real Pi, with a mock model.

usage: python live_pi.py <pi executable> <work dir>

Build mukade first (`cargo build`): the script runs target/debug/mukade from this repository.
<work dir> must not exist yet, or be a previous run's (it holds a `.pi-live-bench` marker):
anything else is refused rather than deleted.

Pi runs with its home, HOME and USERPROFILE inside <work dir>, PI_OFFLINE=1 and a provider whose
baseUrl is a mock Anthropic endpoint on 127.0.0.1 (mock_anthropic.py), which checks every request
against the provider rules and logs it. No real model API is reachable. Written for Windows
(npm `pi.cmd` launchers); the process-tree kill uses taskkill there and a plain kill elsewhere.
See docs/verification/pi-session-and-reasoning-verification.md.
"""
import itertools
import json
import os
import pathlib
import shutil
import subprocess
import sys
import time

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parents[2]
MUKADE = str(REPO / "target" / "debug" / ("mukade.exe" if os.name == "nt" else "mukade"))
FIX = REPO / "fixtures"

PI_EXE = sys.argv[1]
W = pathlib.Path(sys.argv[2])


# ------------------------------------------------------------------------------------ the bench
MARKER = ".pi-live-bench"


def start():
    if W.exists():
        if not (W / MARKER).is_file():
            sys.exit(f"{W} exists and is not a previous run of this script: refusing to delete it")
        shutil.rmtree(W)
    (W / "pi-home").mkdir(parents=True)
    (W / MARKER).write_text("created by live_pi.py" + chr(10))
    (W / "home").mkdir()
    (W / "requests.jsonl").write_text("")
    port_file = W / "port.txt"
    mock = subprocess.Popen(
        [sys.executable, str(HERE / "mock_anthropic.py"), str(port_file), str(W / "requests.jsonl")],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )
    for _ in range(100):
        if port_file.exists() and port_file.read_text().strip():
            break
        time.sleep(0.1)
    port = port_file.read_text().strip()
    (W / "pi-home" / "models.json").write_text(json.dumps({"providers": {"mock": {
        "baseUrl": f"http://127.0.0.1:{port}", "api": "anthropic-messages", "apiKey": "mock-key",
        "models": [{"id": "mock-model", "input": ["text", "image"]}]}}}))
    (W / "pi-home" / "settings.json").write_text(json.dumps(
        {"defaultProvider": "mock", "defaultModel": "mock-model", "quietStartup": True}))
    return mock


def env(extra=None):
    e = dict(os.environ)
    for key in ("ANTHROPIC_API_KEY", "OPENAI_API_KEY", "GEMINI_API_KEY", "OPENROUTER_API_KEY",
                "CLAUDE_CONFIG_DIR", "CODEX_HOME", "OPENCODE_DB", "MUKADE_PI_EXE"):
        e.pop(key, None)
    e.update(PI_CODING_AGENT_DIR=str(W / "pi-home"), PI_OFFLINE="1", PI_SKIP_VERSION_CHECK="1",
             PI_TELEMETRY="0", HOME=str(W / "home"), USERPROFILE=str(W / "home"))
    e.update(extra or {})
    return e


def run(args, cwd=None, extra_env=None, timeout=120):
    """Run a command with stdin closed (Pi reads piped stdin in print mode) and a deadline that
    stops the whole process tree: an npm .cmd launcher leaves its node child holding the pipes."""
    p = subprocess.Popen(args, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                         text=True, encoding="utf-8", errors="replace", cwd=cwd, env=env(extra_env))
    try:
        out, err = p.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        if os.name == "nt":
            subprocess.run(["taskkill", "/PID", str(p.pid), "/T", "/F"], capture_output=True)
        else:
            p.kill()
        try:
            out, err = p.communicate(timeout=5)
        except subprocess.TimeoutExpired:
            out, err = "", ""
        return 124, out, "TIMED OUT after %ss: %s" % (timeout, err)
    return p.returncode, out, err


_seen = [0]


def new_requests():
    """The requests the mock logged since the last call, without the mock's own probes."""
    rows = [json.loads(l) for l in (W / "requests.jsonl").read_text(encoding="utf-8").splitlines() if l.strip()]
    fresh, _seen[0] = rows[_seen[0]:], len(rows)
    return fresh


def blocks(message):
    c = message["content"]
    return [{"type": "text", "text": c}] if isinstance(c, str) else c


def texts(request):
    """(role, text) of every text block the model was sent, in order."""
    return [(m["role"], b["text"]) for m in request["body"]["messages"] for b in blocks(m) if b["type"] == "text"]


def alltext(request):
    return "\n".join(t for _, t in texts(request)) + "\n" + json.dumps(request["body"]["messages"])


def mukade(*args, **kw):
    return run([MUKADE, *args], **kw)


def last_json(stdout):
    return json.loads(stdout.strip().splitlines()[-1])


def pi_version():
    return run([PI_EXE, "--version"])[1].strip()


# ------------------------------------------------------------------------------------ scenarios
RESULTS = []


def record(name, ok, detail=""):
    RESULTS.append((name, ok, detail))
    print(("PASS " if ok else "FAIL ") + name + (("  -- " + detail) if detail else ""), flush=True)


def hop_to_pi(label, src, source_args, cwd, resume="LIVE-PROMPT", extra=(), expect="H27-"):
    """Hop a session into Pi, resume it in the real Pi, and check what the model was sent."""
    cwd.mkdir(parents=True, exist_ok=True)
    new_requests()
    code, out, err = mukade("hop", src, "x", "pi", *source_args, "--cwd", str(cwd), "--dest-version", VERSION,
                            "--pi-exe", PI_EXE, "--resume-prompt", resume, *extra)
    if code != 0:
        return record(label, False, f"mukade exit {code}: {(err or out)[:300]}")
    reply = last_json(out)["resume_output"]
    reqs = [r for r in new_requests() if r["path"].startswith("/v1/messages")]
    if len(reqs) != 1:
        return record(label, False, f"{len(reqs)} model requests")
    r = reqs[0]
    ok = reply == "MOCK-REPLY" and not r["violations"] and expect in alltext(r) and resume in alltext(r)
    record(label, ok, f"reply={reply!r} violations={r['violations']} has_{expect!r}={expect in alltext(r)}")
    return r


def scenarios():
    # 1-4: every source harness into Pi, resumed by the real Pi
    claude_home = W / "claude-home" / "projects" / "p"
    claude_home.mkdir(parents=True)
    shutil.copy(FIX / "claude_code/source/c45db7bc-542c-4201-89de-b590c6fdb797.jsonl", claude_home)
    (W / "work1").mkdir()
    new_requests()
    code, out, err = mukade("hop", "claude_code", "c45db7bc", "pi", "--cwd", str(W / "work1"),
                            "--dest-version", VERSION, "--pi-exe", PI_EXE, "--resume-prompt", "LIVE-PROMPT",
                            extra_env={"CLAUDE_CONFIG_DIR": str(W / "claude-home")})
    reqs = [r for r in new_requests() if r["path"].startswith("/v1/messages")]
    ok = code == 0 and len(reqs) == 1 and not reqs[0]["violations"] and "H27-FINAL" in alltext(reqs[0])
    record("claude_code -> pi, resumed by Pi", ok, (err or "")[:200] if not ok else "")
    hop_to_pi("codex -> pi, resumed by Pi", "codex",
              ["--from-file", str(next((FIX / "codex/source").glob("rollout-*.jsonl")))], W / "work2")
    hop_to_pi("opencode export -> pi, resumed by Pi", "opencode",
              ["--from-file", str(FIX / "opencode/source/session-export.json")], W / "work3")
    hop_to_pi("pi -> pi, resumed by Pi", "pi",
              ["--from-file", str(next((FIX / "pi/source").glob("*.jsonl")))], W / "work4")
    hop_to_pi("claude_code -> pi, portable compaction", "claude_code",
              ["--from-file", str(FIX / "claude_code/source/c45db7bc-542c-4201-89de-b590c6fdb797.jsonl")],
              W / "work5", extra=("--compaction", "portable"), expect="[mukade handoff v1]")

    # 5: the project folder is where Pi itself looks (`pi -c` continues the newest session of the cwd)
    awkward = W / "my proj_v1.2 é" / "work"
    awkward.mkdir(parents=True)
    new_requests()
    code, out, err = mukade("hop", "claude_code", "x", "pi", "--from-file",
                            str(FIX / "claude_code/source/c45db7bc-542c-4201-89de-b590c6fdb797.jsonl"),
                            "--cwd", str(awkward), "--dest-version", VERSION, "--pi-exe", PI_EXE)
    new_requests()
    code2, out2, err2 = run([PI_EXE, "-c", "-p", "NEXT-TURN"], cwd=str(awkward))
    reqs = [r for r in new_requests() if r["path"].startswith("/v1/messages")]
    ok = code == 0 and code2 == 0 and len(reqs) == 1 and "H27-FINAL" in alltext(reqs[0]) and "NEXT-TURN" in alltext(reqs[0])
    record("project folder: `pi -c` finds the session of a path with space, _, ., é", ok,
           "" if ok else f"hop={code} pi={code2} requests={len(reqs)} {err2[:150]}")

    # 6: mixed native / text-form results (the result-order fix)
    meta = json.loads(mukade_dump())["meta"]
    mixed = {"meta": meta, "messages": [
        {"role": "user", "content": [{"type": "text", "text": "MIXED-START"}]},
        {"role": "assistant", "end": "tool_use", "content": [
            {"type": "tool_use", "id": "r1", "name": "Read", "input": {"file_path": "a.txt"}, "canonical": "read"},
            {"type": "tool_use", "id": "f1", "name": "Frobnicate", "input": {"x": 1}},
            {"type": "tool_use", "id": "r2", "name": "Read", "input": {"file_path": "b.txt"}, "canonical": "read"}]},
        {"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": "r1", "content": {"text": "R1-OUT"}},
            {"type": "tool_result", "tool_use_id": "f1", "content": {"text": "F1-OUT"}},
            {"type": "tool_result", "tool_use_id": "r2", "content": {"text": "R2-OUT"}}]}]}
    (W / "mixed.json").write_text(json.dumps(mixed))
    write_to_pi("mixed native + text-form tool results", W / "mixed.json", W / "work6", "R2-OUT", must_not=("No result provided",))

    # 7: an aborted step stays aborted: Pi leaves it out of the next request
    aborted = {"meta": meta, "messages": [
        {"role": "user", "content": [{"type": "text", "text": "ABORT-START"}]},
        {"role": "assistant", "end": "aborted", "content": [{"type": "text", "text": "PARTIAL-ABORTED"}]},
        {"role": "user", "content": [{"type": "text", "text": "AFTER-ABORT"}]}]}
    (W / "aborted.json").write_text(json.dumps(aborted))
    write_to_pi("aborted step is left out by Pi", W / "aborted.json", W / "work7", "AFTER-ABORT",
                must_not=("PARTIAL-ABORTED",))

    # 8: failed tool call and unanswered call
    unanswered = {"meta": meta, "messages": [
        {"role": "user", "content": [{"type": "text", "text": "UNANS-START"}]},
        {"role": "assistant", "end": "tool_use", "content": [
            {"type": "tool_use", "id": "u1", "name": "Read", "input": {"file_path": "a.txt"}, "canonical": "read"},
            {"type": "tool_use", "id": "u2", "name": "Read", "input": {"file_path": "b.txt"}, "canonical": "read"}]},
        {"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": "u1", "content": {"text": "U1-OUT"}, "is_error": True}]}]}
    (W / "unanswered.json").write_text(json.dumps(unanswered))
    write_to_pi("failed call + unanswered call", W / "unanswered.json", W / "work8", "U1-OUT")

    # 9: duplicate results and an id Anthropic would refuse
    odd = {"meta": meta, "messages": [
        {"role": "user", "content": [{"type": "text", "text": "ODD-START"}]},
        {"role": "assistant", "end": "tool_use", "content": [
            {"type": "tool_use", "id": "call_x|fc_y", "name": "Read", "input": {"file_path": "a.txt"}, "canonical": "read"}]},
        {"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": "call_x|fc_y", "content": {"text": "ODD-OUT"}},
            {"type": "tool_result", "tool_use_id": "call_x|fc_y", "content": {"text": "ODD-OUT-AGAIN"}}]}]}
    (W / "odd.json").write_text(json.dumps(odd))
    write_to_pi("duplicate result", W / "odd.json", W / "work9", "ODD-OUT", must_not=("ODD-OUT-AGAIN",))

    # 10: an image in a user turn and in a tool result (Claude's shape -> Pi's)
    png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/q842iQAAAABJRU5ErkJggg=="
    img = {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": png}}
    images = {"meta": meta, "messages": [
        {"role": "user", "content": [{"type": "text", "text": "IMG-START"},
                                      {"type": "attachment", "kind": "image", "path": None, "raw": img}]},
        {"role": "assistant", "end": "tool_use", "content": [
            {"type": "tool_use", "id": "i1", "name": "Read", "input": {"file_path": "p.png"}, "canonical": "read"}]},
        {"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": "i1", "content": {"blocks": [{"type": "text", "text": "SHOT"}, img]}}]}]}
    (W / "images.json").write_text(json.dumps(images))
    r = write_to_pi("images in a user turn and a tool result", W / "images.json", W / "work10", "SHOT")
    if r:
        seen = []
        for m in r["body"]["messages"]:
            for b in blocks(m):
                if b["type"] == "image":
                    seen.append((m["role"], "image", b["source"]["type"], b["source"].get("media_type")))
                if b["type"] == "tool_result" and isinstance(b.get("content"), list):
                    seen += [(m["role"], "tool_result", c["type"]) for c in b["content"]]
        print("     blocks the model saw:", seen)


_dump_cache = []


def mukade_dump():
    if not _dump_cache:
        shutil.copy(FIX / "claude_code/source/c45db7bc-542c-4201-89de-b590c6fdb797.jsonl",
                    W / "claude-home" / "projects" / "p")
        code, out, err = mukade("dump", "claude_code", "c45db7bc",
                                extra_env={"CLAUDE_CONFIG_DIR": str(W / "claude-home")})
        _dump_cache.append(out)
    return _dump_cache[0]


def write_to_pi(label, doc, cwd, expect, must_not=()):
    cwd.mkdir(parents=True, exist_ok=True)
    new_requests()
    code, out, err = mukade("write", "pi", str(doc), "--cwd", str(cwd), "--dest-version", VERSION,
                            "--pi-exe", PI_EXE, "--resume-prompt", "LIVE-PROMPT")
    if code != 0:
        record(label, False, f"mukade exit {code}: {(err or out)[:300]}")
        return None
    reqs = [r for r in new_requests() if r["path"].startswith("/v1/messages")]
    if len(reqs) != 1:
        record(label, False, f"{len(reqs)} model requests")
        return None
    r = reqs[0]
    text = alltext(r)
    bad = [s for s in must_not if s in text]
    ok = not r["violations"] and expect in text and not bad
    record(label, ok, f"violations={r['violations']} saw_{expect!r}={expect in text} unexpected={bad}")
    return r


# --------------------------------------------------------- ground truth: what Pi itself sends
class PiSession:
    """Hand-built Pi session: every entry chained on the previous one unless given a parent."""

    def __init__(self, cwd):
        self.rows = [{"type": "session", "version": 3, "id": "11111111-2222-4333-8444-555555555555",
                      "timestamp": "2026-10-28T10:00:00.000Z", "cwd": str(cwd)}]
        self.n = 0
        self.leaf = None

    def _add(self, kind, parent="leaf", **fields):
        self.n += 1
        eid = f"e{self.n:07d}"
        row = {"type": kind, "id": eid, "parentId": self.leaf if parent == "leaf" else parent,
               "timestamp": f"2026-10-28T10:{self.n:02d}:00.000Z", **fields}
        self.rows.append(row)
        self.leaf = eid
        return eid

    def user(self, text, parent="leaf"):
        return self._add("message", parent, message={"role": "user", "content": text, "timestamp": self.n * 1000})

    def assistant(self, text, parent="leaf", stop="stop"):
        usage = {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "totalTokens": 0,
                 "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "total": 0}}
        return self._add("message", parent, message={
            "role": "assistant", "content": [{"type": "text", "text": text}], "api": "anthropic-messages",
            "provider": "mock", "model": "mock-model", "usage": usage, "stopReason": stop,
            "timestamp": self.n * 1000})

    def save(self, path):
        pathlib.Path(path).write_text("\n".join(json.dumps(r) for r in self.rows) + "\n", encoding="utf-8")


def pi_context(path, cwd, prompt="GT-PROMPT"):
    """What the real Pi sends to the model when it resumes this session file."""
    new_requests()
    code, out, err = run([PI_EXE, "-p", "--session", str(path), prompt], cwd=str(cwd))
    reqs = [r for r in new_requests() if r["path"].startswith("/v1/messages")]
    return (code, reqs[0] if reqs else None, err)


_GROUND_TRUTH_RUNS = itertools.count(1)


def context_matches(label, build, expect_gap=False):
    """Build a Pi session, ask Pi what it sends, then ask mukade to rewrite the session for Pi and
    compare: the model must see the same conversation."""
    # A short folder: Pi files the session under `--<the whole path>--`, and Windows stops at 260.
    cwd = W / f"gt{next(_GROUND_TRUTH_RUNS)}"
    cwd.mkdir(parents=True, exist_ok=True)
    s = PiSession(cwd)
    build(s)
    original = cwd / "original.jsonl"
    s.save(original)
    # Pi appends its own reply to the session it resumes: ask it about a copy, so the original
    # stays what the user's session was.
    resumed = cwd / "resumed-by-pi.jsonl"
    shutil.copy(original, resumed)
    code, truth, err = pi_context(resumed, cwd)
    if truth is None:
        return record(label, False, f"Pi did not send a request (exit {code}) {err[:200]}")
    (cwd / "rewrite").mkdir(exist_ok=True)
    new_requests()
    code, out, err = mukade("hop", "pi", "x", "pi", "--from-file", str(original), "--cwd", str(cwd / "rewrite"),
                            "--dest-version", VERSION, "--pi-exe", PI_EXE, "--resume-prompt", "GT-PROMPT")
    mine = [r for r in new_requests() if r["path"].startswith("/v1/messages")]
    if code != 0 or not mine:
        return record(label, False, f"mukade exit {code} {(err or out)[:200]}")
    want, got = texts(truth), texts(mine[0])
    ok = want == got
    if expect_gap:
        record(label + " (known gap)", not ok, "mukade differs from Pi, as expected" if not ok else "no difference")
    else:
        record(label, ok, "" if ok else f"\n       Pi sends:     {want}\n       mukade sends: {got}")


def ground_truth():
    def branch(s):
        a = s.user("ROOT-Q"); s.assistant("ROOT-A")
        fork = s.leaf
        s.user("ABANDONED-Q"); s.assistant("ABANDONED-A")
        s.user("LIVE-Q", parent=fork); s.assistant("LIVE-A")
    context_matches("Pi tree: abandoned branch is not sent", branch)

    def summary(s):
        s.user("ROOT-Q"); s.assistant("ROOT-A")
        fork = s.leaf
        s.user("ABANDONED-Q"); s.assistant("ABANDONED-A")
        bs = s._add("branch_summary", fork, summary="WENT-DOWN-A-BRANCH", fromId=s.leaf)
        s.user("LIVE-Q", parent=bs); s.assistant("LIVE-A")
    context_matches("Pi branch_summary reaches the model in Pi's words", summary)

    def bash(s):
        s.user("Q1"); s.assistant("A1")
        s._add("message", message={"role": "bashExecution", "command": "echo hi", "output": "hi",
                                    "exitCode": 0, "cancelled": False, "truncated": False, "timestamp": 5})
        s._add("message", message={"role": "bashExecution", "command": "secret", "output": "HIDDEN",
                                    "exitCode": 0, "cancelled": False, "truncated": False,
                                    "excludeFromContext": True, "timestamp": 6})
        s._add("message", message={"role": "bashExecution", "command": "false", "output": "",
                                    "exitCode": 1, "cancelled": False, "truncated": False, "timestamp": 7})
        s.user("Q2"); s.assistant("A2")
    context_matches("Pi bashExecution: text, exclusions and exit code", bash)

    def custom(s):
        s.user("Q1"); s.assistant("A1")
        s._add("custom_message", customType="reminder", content="CUSTOM-CONTENT", display=True)
        s.user("Q2"); s.assistant("A2")
    context_matches("Pi custom_message reaches the model", custom)

    def edited(s):
        a = s.user("Q1"); b = s.assistant("A1-ORIGINAL")
        s._add("context_edit", targetId=b, replacement={"content": "A1-EDITED"})
        s.user("Q2"); s.assistant("A2")
    context_matches("Pi context_edit replaces a message", edited)

    def removed(s):
        a = s.user("Q1"); b = s.assistant("A1-REMOVED")
        s._add("context_edit", targetId=b, replacement=None)
        s.user("Q2"); s.assistant("A2")
    context_matches("Pi context_edit removes a message", removed)

    def aborted(s):
        s.user("Q1"); s.assistant("PARTIAL", stop="aborted")
        s.user("Q2"); s.assistant("A2")
    context_matches("Pi aborted turn is left out", aborted)


if __name__ == "__main__":
    mock = start()
    try:
        VERSION = pi_version()
        print(f"Pi under test: {PI_EXE}  version {VERSION}\n", flush=True)
        scenarios()
        print()
        ground_truth()
    finally:
        mock.terminate()
    failed = [n for n, ok, _ in RESULTS if not ok]
    print(f"\n{len(RESULTS) - len(failed)} of {len(RESULTS)} passed" + (f"; failed: {failed}" if failed else ""))
    sys.exit(1 if failed else 0)
