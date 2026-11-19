# Reading a session of any size

No session is too large to read. A session file of a few hundred megabytes is read in a second or two
and a few tens of megabytes of memory, with the commands below or the three `Reader` calls. The
decision is [ADR-0004](adr/ADR-0004-size-agnostic-session-reads.md); the logic is in
[architecture](architecture.md#reading-a-session-of-any-size).

## Why the plain read does not do

`Reader::read` decodes the whole file into JSON values, then builds the session from them. It holds
about **five times the file**: a 230 MiB session takes 612 MiB. Most of a session's bytes are tool
output, which a screen or a title never needs: in a real 10.6 MB Claude Code session every string
cut at 4 KiB keeps 37% of the bytes and every structural field.

## Pick the call

| you want | call | CLI | reads |
|---|---|---|---|
| to resume a session, or name it: its id, project, title, time, parent | `Reader::overview` | `mukade overview <harness> <id>` | the first 128 KiB and last 384 KiB of the file; **constant time** |
| to show the conversation, or search what is shown | `Reader::read_clipped` | `mukade dump <harness> <id> --clip <bytes>` | the file, one record at a time past 16 MiB; strings over the limit cut |
| to search or export every word, in order | `Reader::read_streamed` | `mukade dump <harness> <id> --stream` | the file twice; one entry in memory at a time |
| to move the session to another harness | `hop`, `export`, `Reader::read` | `mukade hop`, `mukade dump` | the whole session; unchanged |

`overview`, `read_clipped` and `read_streamed` have their own code for **Claude Code, Codex and Pi**.
The other harnesses answer all three through a default that reads the session whole; their stores
are databases, not files that grow.

## Resume or name a session: `overview`

```rust
use mukade::reader::{Lookup, for_harness};
use mukade::roots::StoreRoots;
use mukade::HarnessId;

let roots = StoreRoots::from_env();
let reader = for_harness(HarnessId::ClaudeCode)?;
match reader.overview("c45db7bc-542c-4201-89de-b590c6fdb797", &roots) {
    Ok(info) => println!("{:?} in {:?}, parent {:?}", info.title, info.cwd, info.parent),
    Err(error) => match error.downcast_ref::<Lookup>() {
        Some(Lookup::NotFound(why)) => eprintln!("no such session: {why}"),
        Some(Lookup::Ambiguous(why)) => eprintln!("the id is not unique: {why}"),
        _ => return Err(error),
    },
}
```

The row is the one `list_all` gives, field for field (`SessionInfo`). The id is the **whole id** a
listing shows: a prefix is `NotFound`. A subagent session is asked for by its qualified id
(`<parent>.<agent>`).

```powershell
mukade overview claude_code c45db7bc-542c-4201-89de-b590c6fdb797
```

```json
{"id":"c45db7bc-542c-4201-89de-b590c6fdb797","title":"H27-T3 after compaction","cwd":"C:\\Users\\you\\work","timestamp":"2026-10-27T15:29:05.240Z","location":"C:\\Users\\you\\.claude\\projects\\work\\c45db7bc-….jsonl","archived":false,"unreadable":null,"live":null}
```

The same keys as a [`list`](usage.md#the-list-output) row; `parent` and `kind` appear on a session
with a parent. A session the store names but cannot read has its reason in `unreadable`.

**What it does not see.** A title written in the middle of a file, between the two windows, is not
seen: the listing does not see it either, and the two always agree.

## Show or search the conversation: `read_clipped`

```rust
let clipped = reader.read_clipped(id, &roots, 4096)?;   // cut every string at 4 KiB
for entry in &clipped.session.entries { /* the same entries, ids and order as `read` */ }
if clipped.clipping.is_clipped() {
    println!("{} strings cut, {} bytes left out", clipped.clipping.strings, clipped.clipping.bytes_dropped);
}
```

- The result is a `Clipped { session, clipping }`, **not a `Session`**: a clipped session is for
  showing and searching and cannot be handed to a writer by mistake.
- The session has the entries, ids, kinds and order of `read`. Every string is the full one or a
  prefix of it, cut at a UTF-8 boundary. A text built from several blocks is built from the cut
  blocks, so it is shorter, never longer.
- `source_sha256` is the **full** file's digest.
- A limit under 1024 bytes is raised to 1024.
- Up to 16 MiB (`mukade::reader::STREAM_FROM_BYTES`) the file is read whole, as always, and clipped.
  Above it, it is decoded one line at a time and the cut-away text is never held. The session is the
  same either way; `clipping.streamed` says which way it was.
- Codex keeps a tool call's JSON `arguments` and an `exec` script whole: the reader parses them.

```powershell
mukade dump claude_code YOUR_ID --clip 4096 --output C:\work\view.json
```

The document is the [`dump` document](usage.md#mukade-dump) with one more key:

```json
{"meta":{…},"library":{…},"messages":[…],
 "clipped":{"max_string_bytes":4096,"strings":81,"bytes_dropped":189497,"streamed":false}}
```

`write` refuses a document with a `clipped` key.

## Search or export every word: `read_streamed`

```rust
let mut found = 0;
let meta = reader.read_streamed(id, &roots, &mut |entry| {
    // `entry` is what `read(...).entries` holds, one at a time, with its full text.
    for row in &entry.raw {
        if row.to_string().contains("needle") { found += 1; }
    }
    Ok(())                       // an error here stops the read and is returned
})?;
println!("{found} hits in {}", meta.id);
```

The entries arrive in the order of `read`, with its full content, and the returned `Meta` is the
one `read` gives. Memory follows the session's **structure and its biggest entry**, not its text.
The file is read twice; the wall time grows with its size, so show progress for a very large one.
A Pi session that rewrote earlier text (`context_edit`), and Codex, are read whole first.

```powershell
mukade dump claude_code YOUR_ID --stream --output C:\work\entries.jsonl
```

One JSON object per line, an entry each, then the meta:

```json
{"id":"row-0","kind":{"meta":{"kind":"internal"}},"raw":[{"type":"queue-operation","operation":"enqueue",…}]}
{"id":"fa9e158b-…","kind":{"user":{"text":"H27-TOOLS read alpha.txt and beta.txt","attachments":[]}},"raw":[{…the source record…}]}
{"meta":{"harness":"claude_code","harness_version":"2.1.311","id":"c45db7bc-…","source_sha256":"…"}}
```

## Export in the interchange format: `wire::DocumentWriter`

`wire::to_document` builds the whole document in memory. For a session that is too large,
`DocumentWriter` writes the same document one message at a time, to any `io::Write`: the **bytes**
`serde_json::to_writer(out, &to_document(&session))` writes (compact form), holding no message
(one entry is one message; each is built, written and dropped).

A document states its `meta` first, so the writer takes it at the start.
`Reader::read_streamed_with_meta` calls `on_meta` once, **before the first entry**, and returns the
final meta; `finish` refuses to close a document whose opening meta was not the final one (the two
differ only if a fact of the meta, such as a working directory, is longer than 1 KiB).

```rust
use mukade::wire::DocumentWriter;
use std::cell::RefCell;

let writer: RefCell<Option<DocumentWriter<std::io::BufWriter<std::fs::File>>>> = Default::default();
let meta = reader.read_streamed_with_meta(
    id,
    &roots,
    &mut |meta| {
        let file = std::io::BufWriter::new(std::fs::File::create("session.json")?);
        *writer.borrow_mut() = Some(DocumentWriter::new(file, meta)?);
        Ok(())
    },
    &mut |entry| writer.borrow_mut().as_mut().unwrap().entry(&entry),
)?;
writer.into_inner().unwrap().finish(&meta)?;
```

To change each message before it is written (remove reasoning blocks, redact), use `entry_with`; a
caller that builds its messages itself uses `message(&Value)`:

```rust
writer.entry_with(&entry, &mut |message: &mut serde_json::Value| {
    if let Some(content) = message["content"].as_array_mut() {
        content.retain(|block| block["type"] != "thinking");
    }
    Ok(())
})?;
```

A session with branches (`read_all_branches`) closes with `finish_with_branches(&meta, &branches)`;
the branches are held by the caller already. `mukade dump` prints this document pretty; no caller
needs the writer's compact form changed. On the 229 MiB session the document (120 MB) is written
with a peak of 52 MiB in a debug build.

## Measured

A synthetic Claude Code session of 229 MiB (2,600 tool calls whose results of 45 KB sit twice in
each record, as Claude Code writes them), release build, one process per read:

| read | time | peak memory |
|---|---|---|
| `read` (the whole session) | 1.1 s | 612 MiB |
| `read_clipped`, 4 KiB | 1.0 s | 70 MiB |
| `read_streamed` | 1.4 s | 50 MiB |

Run it again: `cargo test --release --test size_agnostic_reads -- --ignored --nocapture`
(`MUKADE_BIG_MODE=full|clipped|streamed`, one per process, for a peak that is that read's alone).

## Errors

A record in the middle of a file that is not JSON is an error naming its line, `<file>:<line>`, in
all three, as in `read`. A **last** line that is not JSON, cut off by a hard kill, is kept as text
(a `Meta` entry of kind `Unfinished`), cut at the limit in a clipped read. No call refuses a session
for its size.
