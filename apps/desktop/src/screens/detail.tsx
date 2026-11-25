// The details of the selected session, in Sigrid's `Inspector` (not modal: the list stays beside it): what can be done with it
// (open its transcript, hop it, export it, resume it, start another harness in its folder), its facts, the work it left on disk, its
// lineage (the sessions it came from and the ones it started) and the rooms where a member is bound to it.
import { useEffect, useMemo, useState } from 'react';
import { Tree, type TreeNode } from '@acaprino/sigrid/assemblies';
import { DescriptionList, ItemList, ItemRow, Note } from '@acaprino/sigrid/components';
import { Inspector, useAppShell } from '@acaprino/sigrid/layout';
import { Badge, Button, Cluster, Column, Icon, Progress, Section, Text } from '@acaprino/sigrid/primitives';
import { boundTo, chatIndex, type RoomMember } from '../chatindex.ts';
import type { Controller } from '../controller.ts';
import { currentInstallation, selectedRow } from '../state.ts';
import { ancestors, childrenText, kindText, mark } from '../tree.ts';
import type { SessionRow } from '../types.ts';
import { atRisk, canStartIn, isHosted, leaf, otherRows, repositoryError, repositoryLine } from '../work.ts';
import { useController } from './hooks.ts';

function Work({ c }: { c: Controller }) {
  const read = c.work;
  if (!read || read.kind === 'loading') return <Progress aria-label="Reading the work on disk" value={null} />;
  if (read.kind === 'error') return <Note tone="error" title={read.message} />;
  const { work } = read;
  return (
    <Column gap="md">
      <Text mono variant="muted">{work.folder}</Text>
      {work.note ? <Note tone="info" title={work.note} /> : null}
      {work.repositories.map((repo) => (
        <Column key={repo.path} gap="xs">
          <Cluster gap="sm">
            <Text variant="strong" title={repo.path}>{leaf(repo.path)}</Text>
            {atRisk(repo) ? <Badge tone="warning" title="Work that exists only in this folder">at risk</Badge> : null}
          </Cluster>
          {repositoryError(repo) ? <Note tone="warning" title={repositoryError(repo)!} /> : <Text variant="muted">{repositoryLine(repo)}</Text>}
          {otherRows(repo).map((other) => (
            other.error
              ? <Note key={other.title} tone="warning" title={`${other.kind} ${other.name}`}>{other.line}</Note>
              : <Text key={other.title} variant="muted" title={other.title}><Text variant="strong">{other.kind}</Text> {other.name} · {other.line}</Text>
          ))}
        </Column>
      ))}
    </Column>
  );
}

/** The sessions it came from, itself and the ones it started: a small tree, the session itself marked, each a press away. */
function lineage(rows: SessionRow[], row: SessionRow): TreeNode[] | null {
  const chain = ancestors(rows, row.id);
  const children = rows.filter((other) => other.parent === row.id && other.id !== row.id);
  if (!chain.length && !children.length) return null;
  const nodeOf = (one: SessionRow, below: TreeNode[]): TreeNode => {
    const marker = mark(one);
    return { id: one.id, name: one.title ?? one.id, icon: marker?.kind === 'fork' ? 'branch' : marker ? 'layers' : 'threads', meta: marker?.text, ...(below.length ? { children: below } : {}) };
  };
  let node = nodeOf(row, children.map((child) => nodeOf(child, [])));
  for (const id of [...chain].reverse()) node = nodeOf(rows.find((other) => other.id === id)!, [node]);
  return [node];
}

function InRooms({ c, harness, id }: { c: Controller; harness: string; id: string }) {
  const [found, setFound] = useState<RoomMember[]>([]);
  useEffect(() => {
    let live = true;
    chatIndex().catch(() => null).then((index) => { if (live) setFound(index ? boundTo(index, harness, id) : []); });
    return () => { live = false; };
  }, [harness, id]);
  if (!found.length) return null;
  return (
    <Section title={`In rooms · ${found.length}`} level={3} icon={<Icon name="threads" />}>
      <ItemList aria-label="Rooms where a member is bound to this session">
        {found.map(({ room, member }) => (
          <ItemRow
            key={`${room.room_id}:${member.member_id}`}
            title={room.name}
            description={`${member.name} · ${member.role}${member.left_at ? ' · left' : ''}${room.closed_at ? ' · closed' : ''}`}
            leading={<Icon name="threads" />}
            onClick={() => c.chat.openRoom(room.room_id, { member: member.member_id })}
          />
        ))}
      </ItemList>
    </Section>
  );
}

export function SessionInspector() {
  const c = useController();
  const s = c.state;
  const layout = useAppShell();
  const installation = currentInstallation(s);
  const row = selectedRow(s);
  const open = s.detailOpen ?? layout.inspector === 'docked';
  c.docked = layout.inspector === 'docked';
  const folder = row?.cwd;
  useEffect(() => {
    if (installation && row && folder) c.readWork(installation.key, row);
    else c.forgetWork();
  }, [c, installation?.key, row?.id, folder]);
  const tree = useMemo(() => (row ? lineage(s.rows, row) : null), [s.rows, row]);

  let body;
  if (!installation || !row) {
    body = installation?.error
      ? <Note tone="error" title={installation.error}>{installation.label}</Note>
      : <Text variant="muted">Pick a session to see its details.</Text>;
  } else {
    const unreadable = Boolean(row.unreadable);
    const parentListed = row.parent !== undefined && s.rows.some((other) => other.id === row.parent);
    body = (
      <Column gap="lg">
        <Cluster gap="sm">
          <Button size="sm" variant={unreadable ? 'outline' : 'default'} disabled={unreadable} onClick={() => { void c.openReader(); }}>Open transcript</Button>
          <Button size="sm" variant="outline" disabled={unreadable} onClick={() => c.startHop()}><Icon name="importExport" />Hop</Button>
          <Button size="sm" variant="outline" disabled={unreadable} onClick={() => { void c.exportSelected(false); }}><Icon name="download" />Export</Button>
          <Button size="sm" variant="outline" disabled={unreadable} title="Export to a file you choose" onClick={() => { void c.exportSelected(true); }}>Save as…</Button>
          <Button size="sm" variant="outline" disabled={unreadable} onClick={() => { void c.resume(installation.key, row.id); }}><Icon name="play" />Resume</Button>
          {canStartIn(row) ? <Button size="sm" variant="default" title="Start another harness in this folder, with a note on what was done" onClick={() => c.startIn(row)}><Icon name="play" />Start in…</Button> : null}
        </Cluster>
        {row.unreadable
          ? isHosted(row)
            ? <Note tone="info" title="Hosted by ChatGPT">The conversation is not on this machine; the work it left is in its folder.</Note>
            : <Note tone="error" title="Cannot be read">{row.unreadable}</Note>
          : null}
        {row.live ? <Note tone="info" title="Open in a harness">This session is open in {installation.label}. A hop or export copies what is on disk now.</Note> : null}
        <DescriptionList
          aria-label="Facts of the session"
          items={[
            { name: 'Harness', value: installation.label },
            { name: 'Id', value: row.id, code: true },
            { name: 'Kind', value: kindText(row) },
            row.parent === undefined ? null : { name: 'Parent', value: parentListed ? row.parent : `${row.parent} (not listed)`, code: true },
            childrenText(s.rows, row.id) ? { name: 'Children', value: childrenText(s.rows, row.id) } : null,
            row.cwd ? { name: 'Folder', value: row.cwd, code: true } : null,
            row.updated ? { name: 'Updated', value: new Date(row.updated).toLocaleString() } : null,
            { name: 'Archived', value: row.archived ? 'yes' : 'no' },
            row.live === undefined ? null : { name: 'Live', value: row.live ? 'yes' : 'no' },
            { name: 'Path', value: row.path, code: true },
            installation.store ? { name: 'Store', value: installation.store, code: true } : null,
            installation.engine ? { name: 'Engine', value: installation.engine, code: true } : null,
          ]}
        />
        {row.cwd ? <Section title="Work on disk" level={3} icon={<Icon name="branch" />}><Work c={c} /></Section> : null}
        {tree ? (
          <Section title="Lineage" level={3} icon={<Icon name="layers" />}>
            <Tree aria-label="Lineage" nodes={tree} selected={row.id} defaultExpanded={[...ancestors(s.rows, row.id), row.id]} onSelect={(id) => { if (id !== row.id) c.pickSession(id); }} />
          </Section>
        ) : null}
        <InRooms c={c} harness={installation.harness} id={row.id} />
      </Column>
    );
  }

  return (
    <Inspector title={row ? (row.title ?? row.id.slice(0, 8)) : 'Details'} open={open} onOpenChange={(next) => c.setDetailOpen(next)}>
      {body}
    </Inspector>
  );
}
