# Subagent sessions — design

Status: decided 2026-11-12 (the user delegated design calls). Facts:
`D:\Projects\horness-launcher\sidecar\runs\musashi-terminal-4444-2379968-0-134389045251963263-0\tmp\claude\c56a38692edb5f5e\40865358-8ab0-4903-8cee-f73bbc95729b\scratchpad\subagents-research.md`
(all [V] in code or on the author's stores unless marked).

## Problem

A subagent is a session an agent spawns (Claude `Agent`, Codex `spawn_agent`, OpenCode `subagent`).
Today:

| gap | effect |
|---|---|
| Claude children are never listed and not readable by id; `--from-file agent-<id>.jsonl` gives 0 messages and the parent's id | a Claude child cannot be exported |
| OpenCode export names the parent `parentID`; the reader looks for `parent_id` only | `Meta.parent` is empty on every real OpenCode read |
| `parent` means "forked from" for Pi but "spawned by" for Codex and OpenCode; nothing says which | callers cannot tell a fork from a child |
| The child's report the parent saw is dropped silently: Claude `attachment` task-notification rows, Codex `agent_message` rows, OpenCode `synthetic` subagent reports are read as Meta | a hopped parent loses what its subagents told it, and the manifest does not say so |
| CLI `list` cannot show children or a parent | |
| `catalog/harnesses.json` Codex note says "subagent threads are not linked" | wrong since the reader links them |

## Phase 1 (this branch): read children, say what each link is, keep what the parent saw

1. **Model.** `pub enum ParentKind { Fork, Spawn { call_id: Option<String>, agent: Option<String> } }`
   (in `src/model`). New fields `Meta.parent_kind: Option<ParentKind>` and
   `SessionInfo.parent_kind: Option<ParentKind>`, set whenever `parent` is. `Meta.parent`'s doc says
   "the session this one came from; `parent_kind` says how; not carried into a hop". The dump
   document (`src/wire.rs`) carries `parent_kind`.
2. **Readers.**

   | harness | `parent` | `parent_kind` |
   |---|---|---|
   | Claude | child: the parent session id | `Spawn { call_id: meta.toolUseId, agent: meta.agentType }` |
   | Codex | `parent_thread_id` (top level or `source.subagent.thread_spawn`), else `session_id` ≠ `id`; listing: DB edge | `Spawn { call_id: None, agent: agent_role }`; a guardian review thread `agent: "guardian_review"` when identifiable |
   | OpenCode | `parentID` (export) or `parent_id` (rows) | `Spawn { call_id: None, agent: None }`; `info.fork` (verify the field name in OpenCode 2.2.5 source) → `Fork`, else not read |
   | Pi | `parentSession` | `Fork` |

3. **Claude children.**
   - Id: `<parent-id>.<agentId>` (the agentId is unique only inside its parent). `read` accepts it
     (exact, no prefix matching for children); `list_all` lists every child that has a transcript
     (`<project>/<parent-id>/subagents/agent-<agentId>.jsonl`), title from meta.json `description`.
     `list` (roots) is unchanged.
   - Reading a child file (by id or `--from-file`): its `isSidechain` rows are its content, not Meta;
     `meta.id` is the qualified id; `parent` the row's `sessionId`.
   - meta.json without a transcript: not listed. Teammate (`in_process_teammate`, no `toolUseId`):
     `call_id: None`. `forked-skill` files: not listed (open item).
4. **What the parent saw stays in the parent.** A child's report that the parent's model received
   is read as a `User` turn with its text, in place, where the text is readable:
   Claude `attachment` task-notification rows (as the plain-row form already is), OpenCode
   `synthetic` rows with `metadata.source: "subagent"`, Codex `agent_message` rows whose content is
   plaintext. An encrypted one stays Meta and is counted: new manifest count
   `dropped_subagent_messages`, listed in `losses`.
5. **CLI.** `mukade list <harness> --children` lists children too (through `list_all`); each row
   (text and JSON forms) shows `parent` and `kind` (`spawn` / `fork`). Without the flag, output is
   unchanged except the added fields where a root has a parent (Pi forks).
6. **Catalog.** Codex `lineage` note: forks are not recorded; subagent threads name their spawner
   (`session_meta.parent_thread_id`). `recorded` stays `false` (lineage = forks; the agreement test
   holds).

## Phase 2 (next plan, after phase 1 lands): carry children through a hop

Decided direction, to be planned on its own:
- `dump --children` puts each readable child in the dump document (`children: [...]`).
- `hop --children` writes each readable child as its own destination session right after the parent.
  Native link where the destination has one: OpenCode `info.parentID` (import parent first, then each
  child), Claude `<parent>/subagents/agent-<id>.jsonl` + `meta.json` [U: whether Claude shows it].
  Codex and Pi: unlinked sessions. The parent's text-form spawn call names the destination child id;
  the manifest maps source child → destination child.
- The spawn call itself stays text form (L14: the destination's agent types are not the source's).

## Out of scope

Claude forked-skill files and `workflows/`; Codex encrypted child prompts; Codex `thread_spawn_edges`
rows whose rollout is gone (listed today by `list_all`, unchanged); TUI nesting (TUI phase).

## Tests

Fixtures for each harness (Claude: a parent with a `subagents/` folder, meta.json, a foreground and a
background child, an attachment task-notification; OpenCode: export form with `parentID` and a
synthetic report; Codex: a child rollout with `parent_thread_id` and a plaintext and an encrypted
`agent_message`; Pi: a fork). CLI: `list --children` text and JSON; `dump <parent>.<agentId>`.
