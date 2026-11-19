//! No session is too large to read (docs/adr/ADR-0004-size-agnostic-session-reads.md): the
//! overview of one session, a read that clips long strings and never refuses for size, and the
//! full text of a session's entries one at a time.
//!
//! The invariants are checked over every Claude Code, Codex and Pi fixture, and over copies of
//! them whose tool output is inflated so that the clipping has something to cut, both for a file
//! read whole then clipped and for one decoded a record at a time (a threshold forced to zero).

use mukade::model::{Entry, HarnessId, Meta, Session};
use mukade::reader::clip::{Clip, Clipped};
use mukade::reader::{Lookup, claude_code, codex, pi};
use mukade::roots::StoreRoots;
use serde_json::Value;
use std::path::{Path, PathBuf};

mod support;

type Streamed = fn(&Path, &mut dyn FnMut(Entry) -> anyhow::Result<()>) -> anyhow::Result<Meta>;

/// A harness's reads of one file.
struct Api {
    name: &'static str,
    read: fn(&Path) -> anyhow::Result<Session>,
    clipped: fn(&Path, &Clip) -> anyhow::Result<Clipped>,
    streamed: Option<Streamed>,
}

const CLAUDE: Api = Api {
    name: "claude",
    read: claude_code::read_file,
    clipped: claude_code::read_file_clipped,
    streamed: Some(claude_code::read_file_streamed),
};
const CODEX: Api = Api {
    name: "codex",
    read: codex::read_file,
    clipped: codex::read_file_clipped,
    streamed: None,
};
const PI: Api = Api {
    name: "pi",
    read: pi::read_file,
    clipped: pi::read_file_clipped,
    streamed: Some(pi::read_file_streamed),
};

fn fixture(harness: HarnessId) -> PathBuf {
    support::source_path(harness)
}

fn scratch(tag: &str) -> PathBuf {
    support::scratch(HarnessId::ClaudeCode, tag)
}

// --- comparing a full read with a clipped one -----------------------------------------------

/// What `clipped` may differ in from `full`: strings only, each a prefix. `strict` asks for the
/// exact cut (the longest prefix within `max` bytes at a character boundary) when the full string
/// is longer than `max`; a text a reader built from several clipped parts is not one prefix.
fn same_but_clipped(
    full: &Value,
    clipped: &Value,
    max: usize,
    strict: bool,
    at: &str,
    loose: &mut u32,
) {
    match (full, clipped) {
        (Value::String(f), Value::String(c)) => {
            if f == c {
                return;
            }
            if strict {
                assert!(
                    f.starts_with(c.as_str()),
                    "{at}: {c:?} is not a prefix of {f:?}"
                );
                assert!(f.len() > max, "{at}: a string within the limit was cut");
                assert!(c.len() <= max, "{at}: cut at {} > {max}", c.len());
                assert!(
                    !f.is_char_boundary(c.len() + 1) || c.len() + 1 > max || f.len() == c.len(),
                    "{at}: not cut as late as it could be"
                );
            } else if !f.starts_with(c.as_str()) {
                // Built from several clipped parts: shorter, never longer.
                assert!(
                    c.len() < f.len(),
                    "{at}: a clipped text is longer than the full one"
                );
                *loose += 1;
            }
        }
        (Value::Array(f), Value::Array(c)) => {
            assert_eq!(f.len(), c.len(), "{at}: array length");
            for (i, (f, c)) in f.iter().zip(c).enumerate() {
                same_but_clipped(f, c, max, strict, &format!("{at}[{i}]"), loose);
            }
        }
        (Value::Object(f), Value::Object(c)) => {
            let keys = |m: &serde_json::Map<String, Value>| m.keys().cloned().collect::<Vec<_>>();
            assert_eq!(keys(f), keys(c), "{at}: keys");
            for (key, f) in f {
                same_but_clipped(f, &c[key], max, strict, &format!("{at}.{key}"), loose);
            }
        }
        _ => assert_eq!(full, clipped, "{at}"),
    }
}

/// `clipped` is `full` read with strings cut: the same entries in the same order; the source
/// records in `raw` cut exactly; everything derived from them the same but for shorter strings.
fn assert_clipped_of(full: &Session, clipped: &Clipped, max: usize, what: &str) -> u32 {
    let got = &clipped.session;
    assert_eq!(got.entries.len(), full.entries.len(), "{what}: entry count");
    assert_eq!(
        got.branches.len(),
        full.branches.len(),
        "{what}: branch count"
    );
    let mut loose = 0;
    for (i, (f, c)) in full.entries.iter().zip(&got.entries).enumerate() {
        assert_eq!(f.id, c.id, "{what}: entry {i} id");
        let (fk, ck) = (
            serde_json::to_value(&f.kind).unwrap(),
            serde_json::to_value(&c.kind).unwrap(),
        );
        same_but_clipped(
            &fk,
            &ck,
            max,
            false,
            &format!("{what}: entry {i} kind"),
            &mut loose,
        );
        assert_eq!(f.raw.len(), c.raw.len(), "{what}: entry {i} raw count");
        for (j, (f, c)) in f.raw.iter().zip(&c.raw).enumerate() {
            same_but_clipped(
                f,
                c,
                max,
                true,
                &format!("{what}: entry {i} raw {j}"),
                &mut 0,
            );
        }
    }
    let (fm, cm) = (
        serde_json::to_value(&full.meta).unwrap(),
        serde_json::to_value(&got.meta).unwrap(),
    );
    same_but_clipped(&fm, &cm, max, false, &format!("{what}: meta"), &mut loose);
    assert_eq!(
        full.meta.source_sha256, got.meta.source_sha256,
        "{what}: digest"
    );
    loose
}

fn check_file(api: &Api, path: &Path, clip: Clip, expect_clipped: Option<bool>) {
    let what = format!(
        "{} {}",
        api.name,
        path.file_name().unwrap().to_string_lossy()
    );
    let full = (api.read)(path).unwrap_or_else(|e| panic!("{what}: the full read failed: {e:#}"));
    let whole = (api.clipped)(path, &clip.streaming_from(u64::MAX)).expect("clipped, whole");
    let streamed = (api.clipped)(path, &clip.streaming_from(0)).expect("clipped, streamed");
    assert!(!whole.clipping.streamed, "{what}");
    assert!(
        streamed.clipping.streamed || full.entries.is_empty() || api.name == "codex",
        "{what}"
    );
    assert_clipped_of(
        &full,
        &whole,
        clip.max_string_bytes,
        &format!("{what} (whole)"),
    );
    assert_clipped_of(
        &full,
        &streamed,
        clip.max_string_bytes,
        &format!("{what} (streamed)"),
    );
    // The two ways to the clipped records give one session, and cut the same.
    assert_eq!(
        whole.session, streamed.session,
        "{what}: whole and streamed differ"
    );
    assert_eq!(
        whole.clipping.strings, streamed.clipping.strings,
        "{what}: strings cut"
    );
    assert_eq!(
        whole.clipping.bytes_dropped, streamed.clipping.bytes_dropped,
        "{what}: bytes cut"
    );
    if let Some(expected) = expect_clipped {
        assert_eq!(
            whole.clipping.is_clipped(),
            expected,
            "{what}: clipped or not"
        );
    }
}

// --- inflating a fixture so that its tool output is long ------------------------------------

const PAD_KEYS: &[&str] = &[
    "text",
    "content",
    "output",
    "toolUseResult",
    "stdout",
    "stderr",
    "aggregated_output",
    "formatted_output",
];

fn pad_text() -> String {
    "\n é日本 padding of tool output, line after line. ".repeat(80)
}

/// Keys whose text names something (a kind, an id, a path) and is left alone.
const NAMES: &[&str] = &[
    "type",
    "id",
    "tool_use_id",
    "name",
    "role",
    "call_id",
    "uuid",
    "file_path",
    "filePath",
    "signature",
    "status",
    "encrypted_content",
    "model",
    "mime_type",
    "media_type",
];

fn inflate(value: &mut Value, under_key: bool, key: &str) {
    match value {
        Value::String(text) if under_key && !NAMES.contains(&key) => text.push_str(&pad_text()),
        Value::Array(items) => items
            .iter_mut()
            .for_each(|item| inflate(item, under_key, key)),
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                inflate(child, under_key || PAD_KEYS.contains(&key.as_str()), key);
            }
        }
        _ => {}
    }
}

/// A copy of the JSON Lines file `from` at `to` with the text under the padded keys inflated to
/// several kilobytes. The rows keep their order and structure.
fn inflated_copy(from: &Path, to: &Path) {
    let text = std::fs::read_to_string(from).expect("read a fixture");
    let mut out = String::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let mut row: Value = serde_json::from_str(line).expect("a fixture row");
        // Tool-call text that a reader parses (Codex's arguments, the script of an exec) is kept.
        if let Some(payload) = row.get_mut("payload").and_then(Value::as_object_mut) {
            let kept: Vec<(String, Value)> = ["arguments", "input"]
                .iter()
                .filter_map(|key| payload.get(*key).cloned().map(|v| (key.to_string(), v)))
                .collect();
            inflate(&mut row, false, "");
            if let Some(payload) = row.get_mut("payload").and_then(Value::as_object_mut) {
                for (key, value) in kept {
                    payload.insert(key, value);
                }
            }
        } else {
            inflate(&mut row, false, "");
        }
        out.push_str(&row.to_string());
        out.push('\n');
    }
    std::fs::create_dir_all(to.parent().unwrap()).unwrap();
    std::fs::write(to, out).expect("write an inflated copy");
}

fn subagent_transcripts() -> Vec<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/claude_code/subagents");
    let mut files = support::files_under(&root);
    files.retain(|f| f.extension().is_some_and(|e| e == "jsonl"));
    files
}

// --- clipped reads ---------------------------------------------------------------------------

#[test]
fn a_clipped_read_is_the_full_read_with_strings_cut_on_every_fixture() {
    let sets: [(&Api, PathBuf); 3] = [
        (&CLAUDE, fixture(HarnessId::ClaudeCode)),
        (&CODEX, fixture(HarnessId::Codex)),
        (&PI, fixture(HarnessId::Pi)),
    ];
    for (api, path) in &sets {
        for max in [1024, 4096, 1 << 20] {
            check_file(api, path, Clip::new(max), None);
        }
    }
    for path in subagent_transcripts() {
        for max in [1024, 4096] {
            if claude_code::read_file(&path).is_err() {
                continue;
            }
            check_file(&CLAUDE, &path, Clip::new(max), None);
        }
    }
}

#[test]
fn a_clipped_read_cuts_the_inflated_fixtures_and_keeps_their_shape() {
    let dir = scratch("inflated");
    let sets: [(&Api, PathBuf); 3] = [
        (&CLAUDE, fixture(HarnessId::ClaudeCode)),
        (&CODEX, fixture(HarnessId::Codex)),
        (&PI, fixture(HarnessId::Pi)),
    ];
    for (api, path) in &sets {
        let copy = dir.join(api.name).join(path.file_name().unwrap());
        inflated_copy(path, &copy);
        for max in [1024, 2048] {
            check_file(api, &copy, Clip::new(max), Some(true));
        }
        let got = (api.clipped)(&copy, &Clip::new(1024).streaming_from(0)).unwrap();
        assert!(
            got.clipping.strings > 10,
            "{}: {:?}",
            api.name,
            got.clipping
        );
        assert!(
            got.clipping.bytes_dropped > 10_000,
            "{}: {:?}",
            api.name,
            got.clipping
        );
    }
    for (i, path) in subagent_transcripts().iter().enumerate() {
        if claude_code::read_file(path).is_err() {
            continue;
        }
        let copy = dir.join("sub").join(format!(
            "{i}-{}",
            path.file_name().unwrap().to_string_lossy()
        ));
        inflated_copy(path, &copy);
        check_file(&CLAUDE, &copy, Clip::new(1024), None);
    }
}

#[test]
fn a_codex_tool_calls_arguments_stay_parseable_when_the_rest_is_clipped() {
    // The reader parses a call's JSON `arguments` and an `exec` script: they are kept whole, so the
    // calls (and the code-mode split) come out as the full read gives them.
    let dir = scratch("codex-arguments");
    let text = std::fs::read_to_string(fixture(HarnessId::Codex)).unwrap();
    let mut widened = 0;
    let mut out = String::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let mut row: Value = serde_json::from_str(line).unwrap();
        let payload = row.get_mut("payload").and_then(Value::as_object_mut);
        if let Some(payload) = payload
            && payload.get("type").and_then(Value::as_str) == Some("function_call")
            && let Some(Value::String(arguments)) = payload.get_mut("arguments")
            && let Ok(Value::Object(mut parsed)) = serde_json::from_str::<Value>(arguments)
        {
            parsed.insert("pad".into(), Value::String("é".repeat(3000)));
            *arguments = Value::Object(parsed).to_string();
            widened += 1;
        }
        out.push_str(&row.to_string());
        out.push_str("\n");
    }
    assert!(
        widened > 0,
        "the fixture has function calls with object arguments"
    );
    let path = dir.join("rollout-widened.jsonl");
    std::fs::write(&path, out).unwrap();
    let full = codex::read_file(&path).unwrap();
    for from in [u64::MAX, 0] {
        let clipped =
            codex::read_file_clipped(&path, &Clip::new(1024).streaming_from(from)).unwrap();
        let calls = |session: &Session| -> Vec<Value> {
            session
                .entries
                .iter()
                .filter_map(|e| match &e.kind {
                    mukade::model::Kind::Step { calls, .. } => Some(calls),
                    _ => None,
                })
                .flatten()
                .map(|call| call.input.clone())
                .collect()
        };
        assert_eq!(calls(&clipped.session), calls(&full), "stream from {from}");
        assert!(calls(&full).iter().any(|input| input.get("pad").is_some()));
    }
}

#[test]
fn a_limit_below_the_floor_is_raised_to_it() {
    assert_eq!(
        Clip::new(10).max_string_bytes,
        mukade::reader::MIN_STRING_BYTES
    );
    assert_eq!(Clip::new(5000).max_string_bytes, 5000);
}

// --- streamed reads --------------------------------------------------------------------------

fn streamed_entries(streamed: Streamed, path: &Path) -> (Vec<Entry>, Meta) {
    let mut entries = Vec::new();
    let meta = streamed(path, &mut |entry| {
        entries.push(entry);
        Ok(())
    })
    .unwrap_or_else(|e| panic!("{}: {e:#}", path.display()));
    (entries, meta)
}

fn check_streamed(api: &Api, path: &Path) {
    let streamed = api.streamed.expect("a harness that streams");
    let full = (api.read)(path).expect("the full read");
    let (entries, meta) = streamed_entries(streamed, path);
    assert_eq!(
        entries.len(),
        full.entries.len(),
        "{}: entry count",
        path.display()
    );
    for (i, (got, want)) in entries.iter().zip(&full.entries).enumerate() {
        assert_eq!(got, want, "{}: entry {i}", path.display());
    }
    assert_eq!(meta, full.meta, "{}: meta", path.display());
}

#[test]
fn streamed_entries_are_the_full_reads_on_every_fixture() {
    check_streamed(&CLAUDE, &fixture(HarnessId::ClaudeCode));
    check_streamed(&PI, &fixture(HarnessId::Pi));
    for path in subagent_transcripts() {
        if claude_code::read_file(&path).is_ok() {
            check_streamed(&CLAUDE, &path);
        }
    }
}

#[test]
fn streamed_entries_are_the_full_reads_on_the_inflated_fixtures() {
    let dir = scratch("streamed-inflated");
    for (api, harness) in [(&CLAUDE, HarnessId::ClaudeCode), (&PI, HarnessId::Pi)] {
        let path = fixture(harness);
        let copy = dir.join(api.name).join(path.file_name().unwrap());
        inflated_copy(&path, &copy);
        check_streamed(api, &copy);
    }
}

#[test]
fn the_reader_trait_reads_clipped_and_streamed_by_id() {
    let dir = scratch("trait");
    let mut ids = Vec::new();
    for harness in [HarnessId::ClaudeCode, HarnessId::Codex, HarnessId::Pi] {
        let staged = support::stage_fixture(harness, &dir);
        ids.push((harness, staged));
    }
    let roots = support::roots(&dir);
    for (harness, prefix) in ids {
        let reader = mukade::reader::for_harness(harness).unwrap();
        let full = reader.read(&prefix, &roots).unwrap();
        let clipped = reader.read_clipped(&prefix, &roots, 1024).unwrap();
        assert_clipped_of(&full, &clipped, 1024, &format!("{harness} by id"));
        let mut entries = Vec::new();
        let meta = reader
            .read_streamed(&prefix, &roots, &mut |entry| {
                entries.push(entry);
                Ok(())
            })
            .unwrap();
        assert_eq!(entries, full.entries, "{harness}: streamed entries");
        assert_eq!(meta, full.meta, "{harness}: streamed meta");
    }
}

// --- a damaged or unusual file ---------------------------------------------------------------

fn crlf_with_bom(from: &Path, to: &Path) {
    let text = std::fs::read_to_string(from).unwrap();
    let mut out = String::from('\u{feff}');
    for line in text.lines() {
        out.push_str(line);
        out.push_str("\r\n\r\n");
    }
    std::fs::create_dir_all(to.parent().unwrap()).unwrap();
    std::fs::write(to, out).unwrap();
}

#[test]
fn a_bom_blank_lines_and_crlf_read_alike_whole_and_streamed() {
    let dir = scratch("crlf");
    for (api, harness) in [
        (&CLAUDE, HarnessId::ClaudeCode),
        (&PI, HarnessId::Pi),
        (&CODEX, HarnessId::Codex),
    ] {
        let path = fixture(harness);
        let copy = dir.join(api.name).join(path.file_name().unwrap());
        crlf_with_bom(&path, &copy);
        check_file(api, &copy, Clip::new(1024), None);
        if api.streamed.is_some() {
            check_streamed(api, &copy);
        }
    }
}

#[test]
fn a_cut_off_last_line_is_kept_as_text_and_a_bad_middle_line_is_refused_alike() {
    let dir = scratch("damaged");
    for (api, harness) in [
        (&CLAUDE, HarnessId::ClaudeCode),
        (&PI, HarnessId::Pi),
        (&CODEX, HarnessId::Codex),
    ] {
        let path = fixture(harness);
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
        // A hard kill: the last record is cut in half, long enough to be clipped.
        let cut = format!(
            "{}\n{}{}",
            lines[..lines.len() - 1].join("\n"),
            &lines[lines.len() - 1][..40],
            "é".repeat(2000)
        );
        let file = dir.join(api.name).join("cut.jsonl");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, &cut).unwrap();
        check_file(api, &file, Clip::new(1024), None);
        if api.streamed.is_some() {
            check_streamed(api, &file);
        }
        // A bad line that is not last.
        let mut broken: Vec<&str> = lines.clone();
        broken.insert(3, "{\"not json");
        let file = dir.join(api.name).join("broken.jsonl");
        std::fs::write(&file, broken.join("\n")).unwrap();
        let full = (api.read)(&file)
            .expect_err("a bad middle line is refused")
            .to_string();
        for from in [u64::MAX, 0] {
            let error = (api.clipped)(&file, &Clip::new(1024).streaming_from(from))
                .expect_err("a bad middle line is refused")
                .to_string();
            assert_eq!(
                error, full,
                "{}: the error of the clipped read ({from})",
                api.name
            );
        }
        assert!(full.ends_with(":4"), "{}: {full}", api.name);
    }
}

// --- the overview of one session -------------------------------------------------------------

#[test]
fn the_overview_of_a_session_is_its_row_of_the_listing() {
    let dir = scratch("overview");
    for harness in [HarnessId::ClaudeCode, HarnessId::Codex, HarnessId::Pi] {
        support::stage_fixture(harness, &dir);
    }
    let roots = support::roots(&dir);
    for harness in [HarnessId::ClaudeCode, HarnessId::Codex, HarnessId::Pi] {
        let reader = mukade::reader::for_harness(harness).unwrap();
        let listed = reader.list_all(&roots).unwrap();
        assert!(!listed.is_empty(), "{harness}: nothing listed");
        for row in &listed {
            assert_eq!(
                &reader.overview(&row.id, &roots).unwrap(),
                row,
                "{harness}: {}",
                row.id
            );
        }
    }
}

#[test]
fn the_overview_of_a_subagent_session_is_its_row_of_the_listing() {
    let dir = scratch("overview-sub");
    let roots = support::stage_claude_subagents(&dir);
    let reader = mukade::reader::for_harness(HarnessId::ClaudeCode).unwrap();
    let listed = reader.list_all(&roots).unwrap();
    assert!(
        listed.len() > 2,
        "the fixture lists children: {}",
        listed.len()
    );
    for row in &listed {
        assert_eq!(
            &reader.overview(&row.id, &roots).unwrap(),
            row,
            "{}",
            row.id
        );
    }
}

#[test]
fn the_overview_refuses_what_is_missing_or_repeated() {
    let dir = scratch("overview-refuse");
    support::stage_fixture(HarnessId::ClaudeCode, &dir);
    support::stage_fixture(HarnessId::Pi, &dir);
    let roots = support::roots(&dir);
    let id = support::source_id(HarnessId::ClaudeCode);
    let claude = mukade::reader::for_harness(HarnessId::ClaudeCode).unwrap();
    let pi = mukade::reader::for_harness(HarnessId::Pi).unwrap();
    for (reader, missing) in [(&claude, "no-such-session"), (&pi, "no-such-session")] {
        let error = reader.overview(missing, &roots).expect_err("missing");
        assert!(
            matches!(error.downcast_ref::<Lookup>(), Some(Lookup::NotFound(_))),
            "{error:#}"
        );
    }
    // A prefix is not an id.
    let error = claude.overview(&id[..8], &roots).expect_err("a prefix");
    assert!(
        matches!(error.downcast_ref::<Lookup>(), Some(Lookup::NotFound(_))),
        "{error:#}"
    );
    // The same session file in two projects is two sessions of one id.
    let projects = dir.join("home").join("projects");
    let twin = projects.join("another-project");
    std::fs::create_dir_all(&twin).unwrap();
    std::fs::copy(
        fixture(HarnessId::ClaudeCode),
        twin.join(format!("{id}.jsonl")),
    )
    .unwrap();
    let error = claude.overview(id, &roots).expect_err("repeated");
    assert!(
        matches!(error.downcast_ref::<Lookup>(), Some(Lookup::Ambiguous(_))),
        "{error:#}"
    );
}

#[test]
fn an_unset_store_is_no_store() {
    for harness in [HarnessId::ClaudeCode, HarnessId::Codex, HarnessId::Pi] {
        let reader = mukade::reader::for_harness(harness).unwrap();
        let error = reader
            .overview("x", &StoreRoots::default())
            .expect_err("no store");
        assert!(
            matches!(error.downcast_ref::<Lookup>(), Some(Lookup::NoStore(_))),
            "{harness}: {error:#}"
        );
    }
}

#[test]
fn a_session_the_default_reads_is_clipped_after_it_is_read() {
    // The harnesses without a clipped read of their own: the session is read whole, then clipped.
    for harness in [
        HarnessId::OpenCode,
        HarnessId::Hermes,
        HarnessId::DeepSeekHarness,
        HarnessId::ClaudeCode,
    ] {
        let session = support::read_source(harness);
        let same = mukade::reader::clip_session(&session, usize::MAX / 2).unwrap();
        assert_eq!(
            same.session, session,
            "{harness}: the session does not survive the round trip"
        );
        assert!(!same.clipping.is_clipped());
        let clipped = mukade::reader::clip_session(&session, 1024).unwrap();
        assert_clipped_of(&session, &clipped, 1024, &format!("{harness} (default)"));
    }
}

// --- a session past the streaming threshold --------------------------------------------------

/// A Claude Code session of `turns` tool calls whose results are `result_bytes` each, in the shape
/// Claude Code writes: a prompt, then per turn an assistant `tool_use` row and a user `tool_result`
/// row that also carries the `toolUseResult` copy of the output.
fn synthetic_claude(path: &Path, turns: usize, result_bytes: usize) -> u64 {
    use std::io::Write;
    let unit = "line of tool output: é日本 0123456789 abcdefghij klmnopqrst\n";
    let output = unit.repeat(result_bytes / unit.len() + 1);
    let session = "5e55105e-0000-4000-8000-000000000001";
    let base = serde_json::json!({
        "isSidechain": false, "userType": "external", "entrypoint": "sdk-cli",
        "cwd": "C:\\work\\big", "sessionId": session, "version": "2.1.311", "gitBranch": "HEAD",
    });
    let with = |extra: Value| {
        let mut row = base.clone();
        row.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        row
    };
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut out = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    let mut parent = String::new();
    let stamp = |n: usize| {
        format!(
            "2026-11-13T10:{:02}:{:02}.{:03}Z",
            (n / 3600) % 60,
            (n / 60) % 60,
            n % 60 * 7
        )
    };
    let mut write = |row: Value| {
        serde_json::to_writer(&mut out, &row).unwrap();
        out.write_all(b"\n").unwrap();
    };
    let prompt = "00000000-0000-4000-8000-000000000000".to_string();
    write(with(serde_json::json!({
        "type": "user", "uuid": prompt, "parentUuid": null, "timestamp": stamp(0),
        "message": {"role": "user", "content": "read all the files"},
    })));
    parent.push_str(&prompt);
    for n in 1..=turns {
        let call = format!("toolu_{n:08}");
        let assistant = format!("a{n:07}-0000-4000-8000-000000000000");
        let result = format!("b{n:07}-0000-4000-8000-000000000000");
        write(with(serde_json::json!({
            "type": "assistant", "uuid": assistant, "parentUuid": parent, "timestamp": stamp(2 * n),
            "message": {"id": format!("msg_{n}"), "type": "message", "role": "assistant",
                "model": "claude-opus-5-5", "stop_reason": "tool_use",
                "content": [{"type": "tool_use", "id": call, "name": "Read",
                    "input": {"file_path": format!("C:\\work\\big\\file-{n}.txt")}}]},
        })));
        write(with(serde_json::json!({
            "type": "user", "uuid": result, "parentUuid": assistant, "timestamp": stamp(2 * n + 1),
            "sourceToolAssistantUUID": assistant,
            "message": {"role": "user", "content": [{"type": "tool_result", "tool_use_id": call, "content": output}]},
            "toolUseResult": {"type": "text", "file": {"filePath": format!("C:\\work\\big\\file-{n}.txt"),
                "content": output, "numLines": 100, "startLine": 1, "totalLines": 100}},
        })));
        parent = result;
    }
    write(with(serde_json::json!({
        "type": "assistant", "uuid": "ffffffff-0000-4000-8000-000000000000", "parentUuid": parent,
        "timestamp": stamp(2 * turns + 2),
        "message": {"id": "msg_end", "type": "message", "role": "assistant", "model": "claude-opus-5-5",
            "stop_reason": "end_turn", "content": [{"type": "text", "text": "read them all"}]},
    })));
    drop(write);
    out.flush().unwrap();
    std::fs::metadata(path).unwrap().len()
}

#[test]
fn a_session_past_the_threshold_is_decoded_a_record_at_a_time_and_reads_alike() {
    let dir = scratch("past-threshold");
    let path = dir.join("home/projects/big/5e55105e-0000-4000-8000-000000000001.jsonl");
    // 18 MiB: past the threshold the library chooses, with no test forcing it.
    let size = synthetic_claude(&path, 300, 30_000);
    assert!(size > mukade::reader::STREAM_FROM_BYTES, "{size}");
    let clipped = claude_code::read_file_clipped(&path, &Clip::new(4096)).unwrap();
    assert!(clipped.clipping.streamed, "{:?}", clipped.clipping);
    assert_eq!(clipped.clipping.strings, 600, "{:?}", clipped.clipping);
    let full = claude_code::read_file(&path).unwrap();
    assert_clipped_of(&full, &clipped, 4096, "past the threshold");
    check_streamed(&CLAUDE, &path);
    // The interchange document, written message by message from the streamed read, is the
    // document of the full read, byte for byte.
    let roots = support::roots(&dir);
    let reader = mukade::reader::for_harness(HarnessId::ClaudeCode).unwrap();
    let writer: std::cell::RefCell<Option<mukade::wire::DocumentWriter<Vec<u8>>>> =
        Default::default();
    let meta = reader
        .read_streamed_with_meta(
            "5e55105e",
            &roots,
            &mut |meta| {
                *writer.borrow_mut() = Some(mukade::wire::DocumentWriter::new(Vec::new(), meta)?);
                Ok(())
            },
            &mut |entry| writer.borrow_mut().as_mut().unwrap().entry(&entry),
        )
        .unwrap();
    let streamed = writer.into_inner().unwrap().finish(&meta).unwrap();
    let mut whole = Vec::new();
    let by_id = reader.read("5e55105e", &roots).unwrap();
    serde_json::to_writer(&mut whole, &mukade::wire::to_document(&by_id)).unwrap();
    assert!(
        streamed == whole,
        "{} bytes against {}",
        streamed.len(),
        whole.len()
    );
    // The whole session read through the trait never refuses.
    let roots = support::roots(&dir);
    let reader = mukade::reader::for_harness(HarnessId::ClaudeCode).unwrap();
    let through = reader.read_clipped("5e55105e", &roots, 4096).unwrap();
    assert!(
        through.clipping.streamed,
        "the boxed reader runs the reader's own clipped read"
    );
    let (mut a, mut b) = (through.session, clipped.session);
    // The same file, named with other slashes.
    a.meta.source_path = None;
    b.meta.source_path = None;
    assert_eq!(a, b);
}

// --- a session past 100 MiB, measured --------------------------------------------------------

#[cfg(windows)]
fn peak_working_set() -> u64 {
    #[repr(C)]
    struct Counters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> isize;
        fn K32GetProcessMemoryInfo(process: isize, counters: *mut Counters, cb: u32) -> i32;
    }
    let mut counters = Counters {
        cb: std::mem::size_of::<Counters>() as u32,
        page_fault_count: 0,
        peak_working_set_size: 0,
        working_set_size: 0,
        quota_peak_paged_pool_usage: 0,
        quota_paged_pool_usage: 0,
        quota_peak_non_paged_pool_usage: 0,
        quota_non_paged_pool_usage: 0,
        pagefile_usage: 0,
        peak_pagefile_usage: 0,
    };
    // SAFETY: the counters are a live, correctly sized struct of the layout the call fills.
    let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb) };
    assert_ne!(ok, 0, "GetProcessMemoryInfo failed");
    counters.peak_working_set_size as u64
}

#[cfg(not(windows))]
fn peak_working_set() -> u64 {
    0
}

/// `MUKADE_BIG_MODE=full|clipped|streamed|document` picks what one process reads, so that each
/// number is the peak of that read alone. The file is generated once under the system's temporary
/// folder (outside any repository) and reused.
///
/// `cargo test --release --test size_agnostic_reads -- --ignored --nocapture`
#[test]
#[ignore = "generates a 100+ MiB session; run in release with --ignored"]
fn a_session_past_100_mib_is_read_clipped_in_bounded_memory() {
    let dir = std::env::temp_dir().join("mukade-big-session");
    let path = dir.join("projects/big/5e55105e-0000-4000-8000-000000000001.jsonl");
    if !path.is_file() {
        // 2,600 results of 45 KB, each held twice (`tool_result` and `toolUseResult`).
        synthetic_claude(&path, 2_600, 45_000);
    }
    let size = std::fs::metadata(&path).unwrap().len();
    assert!(size > 100 * 1024 * 1024, "{size}");
    let mode = std::env::var("MUKADE_BIG_MODE").unwrap_or_else(|_| "clipped".into());
    let before = peak_working_set();
    let started = std::time::Instant::now();
    let summary = match mode.as_str() {
        "full" => {
            let session = claude_code::read_file(&path).unwrap();
            format!("{} entries", session.entries.len())
        }
        "clipped" => {
            let clipped = claude_code::read_file_clipped(&path, &Clip::new(4096)).unwrap();
            assert!(clipped.clipping.streamed);
            format!(
                "{} entries, {:?}",
                clipped.session.entries.len(),
                clipped.clipping
            )
        }
        "streamed" => {
            let (mut count, mut bytes) = (0usize, 0usize);
            let meta = claude_code::read_file_streamed(&path, &mut |entry| {
                count += 1;
                bytes += entry
                    .raw
                    .iter()
                    .map(|row| row.to_string().len())
                    .sum::<usize>();
                Ok(())
            })
            .unwrap();
            assert!(meta.source_sha256.is_some());
            format!("{count} entries, {bytes} bytes of records delivered")
        }
        "trait" => {
            let roots = StoreRoots {
                claude_config_dir: Some(dir.clone()),
                ..StoreRoots::default()
            };
            let reader = mukade::reader::for_harness(HarnessId::ClaudeCode).unwrap();
            let mut count = 0usize;
            reader
                .read_streamed("5e55105e", &roots, &mut |_| {
                    count += 1;
                    Ok(())
                })
                .unwrap();
            format!("{count} entries by the trait")
        }
        "messages" => {
            let mut count = 0usize;
            claude_code::read_file_streamed(&path, &mut |entry| {
                count += usize::from(mukade::wire::message_of(&entry, 0).is_some());
                Ok(())
            })
            .unwrap();
            format!("{count} messages built")
        }
        "document" => {
            // The interchange document, written message by message to a sink that counts.
            struct Count(usize);
            impl std::io::Write for Count {
                fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                    self.0 += bytes.len();
                    Ok(bytes.len())
                }
                fn flush(&mut self) -> std::io::Result<()> {
                    Ok(())
                }
            }
            let roots = StoreRoots {
                claude_config_dir: Some(dir.clone()),
                ..StoreRoots::default()
            };
            let reader = mukade::reader::for_harness(HarnessId::ClaudeCode).unwrap();
            let writer: std::cell::RefCell<Option<mukade::wire::DocumentWriter<Count>>> =
                Default::default();
            let meta = reader
                .read_streamed_with_meta(
                    "5e55105e",
                    &roots,
                    &mut |meta| {
                        *writer.borrow_mut() =
                            Some(mukade::wire::DocumentWriter::new(Count(0), meta)?);
                        Ok(())
                    },
                    &mut |entry| writer.borrow_mut().as_mut().unwrap().entry(&entry),
                )
                .unwrap();
            let count = writer.into_inner().unwrap().finish(&meta).unwrap();
            format!("{} bytes of document written", count.0)
        }
        other => panic!("MUKADE_BIG_MODE {other:?}"),
    };
    let elapsed = started.elapsed();
    println!(
        "BIG {mode}: file {} MiB, {summary}, {:.2?}, peak working set {} MiB (before the read: {} MiB)",
        size / (1024 * 1024),
        elapsed,
        peak_working_set() / (1024 * 1024),
        before / (1024 * 1024),
    );
}

// --- a boxed reader runs the reader's own code ------------------------------------------------

/// A reader that overrides the three reads and nothing else.
struct Probe;

impl mukade::reader::Reader for Probe {
    fn harness(&self) -> HarnessId {
        HarnessId::Pi
    }
    fn list(&self, _: &StoreRoots) -> anyhow::Result<Vec<mukade::reader::SessionInfo>> {
        anyhow::bail!("the default ran: list")
    }
    fn read(&self, _: &str, _: &StoreRoots) -> anyhow::Result<Session> {
        anyhow::bail!("the default ran: read")
    }
    fn overview(&self, id: &str, _: &StoreRoots) -> anyhow::Result<mukade::reader::SessionInfo> {
        Ok(mukade::reader::SessionInfo {
            id: id.into(),
            ..Default::default()
        })
    }
    fn read_clipped(&self, _: &str, _: &StoreRoots, max: usize) -> anyhow::Result<Clipped> {
        anyhow::bail!("own clipped {max}")
    }
    fn read_streamed_with_meta(
        &self,
        _: &str,
        _: &StoreRoots,
        _: &mut dyn FnMut(&Meta) -> anyhow::Result<()>,
        _: &mut dyn FnMut(Entry) -> anyhow::Result<()>,
    ) -> anyhow::Result<Meta> {
        anyhow::bail!("own streamed")
    }
}

#[test]
fn a_boxed_or_borrowed_reader_forwards_the_three_reads() {
    // The adapters hand out `&'static dyn Reader`, which `for_harness` boxes.
    let probe = Probe;
    let boxed: Box<dyn mukade::reader::Reader> = Box::new(&probe);
    let roots = StoreRoots::default();
    assert_eq!(boxed.overview("x", &roots).unwrap().id, "x");
    let error = boxed
        .read_clipped("x", &roots, 2048)
        .unwrap_err()
        .to_string();
    assert_eq!(error, "own clipped 2048");
    let mut noop = |_: Entry| Ok(());
    // `read_streamed` reaches the reader's `read_streamed_with_meta` through the default.
    let error = boxed
        .read_streamed("x", &roots, &mut noop)
        .unwrap_err()
        .to_string();
    assert_eq!(error, "own streamed");
}
