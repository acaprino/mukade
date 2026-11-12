---
adr_id: ADR-0003
version: 1
title: Shared local MCP chat service with a reusable library core
status: Proposed
author: Alfio Caprino
scope:
  paths:
    - src/bus/**
    - src/chat/**
    - src/cli/chat.rs
    - src/reader/origin.rs
  modules:
    - "mukade::bus"
    - "mukade::chat"
    - "mukade::reader::origin"
  domains:
    - local-mcp-chat
governs_symbols:
depends_on:
  - ADR-0001
created_from_task: local-mcp-chat
proposed_on: 2026-11-11
proposed_at_sha: ba02882
accepted_on: null
accepted_at_sha: null
last_verified_sha: null
retired_on: null
retired_reason: null
supersedes:
superseded_by: null
changes:
---

# ADR-0003: Shared local MCP chat service with a reusable library core

## Context and Problem Statement

Several local harness instances need dialogue, worker supervision and reduced observation.
The user chose one shared server, started in the background with `mukade chat start`, inspected
with `mukade chat status`, and stopped with `mukade chat stop`. Conversation state must survive
its lifecycle. Existing native-session readers and domain foundations should serve that design.

The authoritative requirements are in the
[MCP chat specification](../superpowers/specs/2026-11-11-mcp-chat-design.md).

## Decision Drivers

- One local service shared by multiple harnesses, including multiple instances of one harness.
- Structured MCP tools for agents and a CLI for the user, backed by the same library rules.
- Persistent history and explicit delivery receipts across disconnects and restarts.
- Explicit native-session consent and a reduced view that excludes tool bodies and outputs.
- No change to the existing session-interchange and installation-discovery commands.

## Decision

Keep domain logic and persistence in the Rust library. Add a service layer, `mukade::chat`,
using a maintained MCP SDK and Streamable HTTP bound to loopback. Necessary SDK/HTTP/runtime
dependencies are allowed after compatibility validation. Deploy it in the same executable.
The server exclusively owns chat persistence; MCP and CLI clients never open its SQLite file.

Lifecycle uses an OS process lock, authenticated readiness/status/shutdown and atomically
published instance metadata. The data directory follows mukade's portable policy and must
remain outside projects and native stores. Only explicit connection-configuration operations
may modify the selected harness's integration configuration; native transcripts are read-only.
The exclusions in section 0 of the session-interchange specification remain applicable.
Persist a service identity across restarts and use a fresh instance identity for each process.
Clients validate both for their respective connection and lifecycle operations. Pin MCP
revisions explicitly because connection establishment and cancellation differ by revision.

Rooms contain observer, peer, lead and worker members. Only observers may observe a bound
native session. Bindings use the instance's explicit full native ID and a binding generation.
Rejoin rotates the member handle and fences the old process. Unbind and leave revoke consent
even in closed rooms. Unknown provenance and unsupported tools contribute no raw content.
Permitted conversational text follows masking and size limits and may include quoted material.

Room administration, historical queries/exports and explicit member recovery use the shared
local service credential.
Member operations enforce the role matrix. This is a cooperative, single-account service,
and configured clients possess its administrative authority. Delete requires a selected
closed room and its reviewed revision. A caller's confirmation cannot prove human approval.

Read/wait/observe stage batches without advancing committed cursors. An explicit, idempotent
receipt acknowledges a batch, separately from model processing. Mutations use application
request IDs to avoid duplicate events when clients retry a committed operation. The service
never treats a successful socket flush as evidence of remote receipt.
Pending batches take precedence over new wait filters, and a repeated old acknowledgement
cannot consume a newer batch. Workers' signals address the instruction's author and track
state separately for each worker and instruction. Growth
checks on free disk space permit cleanup and receipt operations below the policy threshold.

## Verified Interview Examples

- **EX-001**: the user starts one background service and connects several harnesses to its endpoint.
- **EX-002**: a Claude Code peer binds its native ID; a Codex observer explains its reduced session
  view, separating stated reasons from interpretations and sending no messages to the peer.
- **EX-003**: peers exchange tagged room messages; a lead sends addressed instructions and workers
  signal done/blocked against the instruction event numbers.
- **EX-004**: two instances with the same harness and working directory remain separate members;
  an absent native ID never results in a guessed binding.
- **EX-005**: stopping and restarting the service preserves rooms, messages and pending progress.

## Rejected Alternatives

- **RA-001**: Independent CLI processes coordinating through the database: spreads storage access and
  delivery coordination across participants instead of using the selected shared service.
- **RA-002**: One independent stdio server per harness: does not supply the single shared process selected
  by the user. A compatibility bridge may forward to that process if a tested client needs it.
- **RA-003**: Reimplementing MCP/HTTP manually to keep the dependency count fixed: increases protocol and
  lifecycle maintenance; a vetted SDK is preferred.
- **RA-004**: Whole-session observation or native-store message injection: violates the content boundary
  and the source-session ownership rule.
- **RA-005**: Treating notifications as proof that a model has read or acted: behaviour belongs to the
  host integration and must be measured separately.


## Enforceable Invariants

- **INV-001**: one service owns each canonical chat data directory; clients use its endpoint.
- **INV-002**: native transcripts are never written; configuration changes require the explicit
  connect/disconnect operation and preserve unrelated settings.
- **INV-003**: observation requires an observer, an explicit target binding and the reduced view.
- **INV-004**: unacknowledged delivery never advances the committed cursor; retries are identifiable.
- **INV-005**: messages and signals are checked before persistence; reduced fields are masked before
  truncation; recognized sensitive values do not enter their corresponding output/storage path.
- **INV-006**: stop/crash does not expire or delete rooms, members or committed history.
- **INV-007**: local HTTP credentials are protected operational state, never conversation content.
- **INV-008**: other mukade capabilities retain their public behaviour.
- **INV-009**: revoked bindings and superseded member handles cannot authorize later operations.
- **INV-010**: retries preserve the original committed result and cannot consume newer delivery.

## Consequences

The service adds startup/readiness/shutdown, protocol negotiation, cancellation, endpoint
security and reconnect behaviour. Clients need endpoint access rather than database write
permission. Automatic message insertion and own-session-ID discovery still need verification
per harness. Existing projection, role, ledger and transaction code is reusable; in-process
cursor consumption must be adapted to durable remote receipts before serving MCP clients.

## Confirmation

Verify lifecycle ownership and stale metadata, concurrent clients, role matrices, exact native
identity, secrets, duplicate mutation retries, dropped responses, receipt recovery, growing
transcripts, configuration preservation and restart with pending events. Live acceptance uses
two real simultaneous harnesses and states the tested versions and integration limits.
The specification's acceptance table covers the required race, retry and low-space cases.
This record remains Proposed until its text is accepted; it does not assert that the service
has been implemented.
