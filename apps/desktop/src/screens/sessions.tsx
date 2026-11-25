// The sessions of the installation: Sigrid's `Tree` (only the rows in sight are in the page) with a filter above it, and beside it,
// in a `SplitPane` the person resizes, the transcript of the selected session. The tree is fed by `tree.ts` (the rows, newest first,
// each under its parent); the model is the controller's. Enter opens the transcript, Space folds a parent, j and k move.
import { useEffect, useMemo, useState, type Ref } from 'react';
import type { TreeNode } from '@acaprino/sigrid/assemblies';
import { Tree } from '@acaprino/sigrid/assemblies';
import { Empty, Note, SearchField } from '@acaprino/sigrid/components';
import { Panel, SplitPane } from '@acaprino/sigrid/layout';
import { Badge, Button, Cluster, Progress, Text } from '@acaprino/sigrid/primitives';
import { currentInstallation, visiblePlaces } from '../state.ts';
import { foldedAfter, mark, nest, openBranches, placed, shortTime, type Nested } from '../tree.ts';
import { isHosted } from '../work.ts';
import { useAfter, useController } from './hooks.ts';
import { Reader } from './reader.tsx';

const SPLIT_KEY = 'mukade.split.sessions';

/** The row of a session: its title, what it is to its parent, and at its end its states and when it was last used. */
function nodeOf(item: Nested): TreeNode {
  const { row } = item.place;
  const marker = mark(row);
  const meta = (
    <Cluster gap="xs" wrap={false}>
      {marker && marker.kind !== 'fork' ? <Badge variant="outline">{marker.text}</Badge> : null}
      {row.live ? <Badge tone="success">live</Badge> : null}
      {row.archived ? <Badge variant="secondary">archived</Badge> : null}
      {isHosted(row) ? <Badge variant="secondary" title={row.unreadable}>hosted</Badge> : row.unreadable ? <Badge tone="danger" title={row.unreadable}>unreadable</Badge> : null}
      <Text variant="muted" title={row.updated ?? ''}>{shortTime(row.updated)}</Text>
    </Cluster>
  );
  return {
    id: row.id,
    name: row.title ?? `${row.id.slice(0, 8)} (no title)`,
    icon: marker?.kind === 'fork' ? 'branch' : marker ? 'layers' : 'threads',
    meta,
    ...(item.children ? { children: item.children.map(nodeOf) } : {}),
  };
}

/** The ids in the order the list shows them: what j and k step through. */
function shownIds(nodes: readonly Nested[], open: ReadonlySet<string>): string[] {
  return nodes.flatMap((node) => [node.place.row.id, ...(node.children && open.has(node.place.row.id) ? shownIds(node.children, open) : [])]);
}

function SessionList({ field }: { field: Ref<HTMLInputElement> }) {
  const c = useController();
  const s = c.state;
  const installation = currentInstallation(s);
  const searching = s.query.trim() !== '';
  const nested = useMemo(() => nest(placed(s.rows, s.folder, s.query)), [s.rows, s.folder, s.query]);
  const nodes = useMemo(() => nested.map(nodeOf), [nested]);
  const open = useMemo(() => (searching ? nested.flatMap(function all(node): string[] { return node.children ? [node.place.row.id, ...node.children.flatMap(all)] : []; }) : openBranches(nested, s.collapsed)), [nested, searching, s.collapsed]);
  const count = visiblePlaces(s).length;
  const folder = s.folder === null ? 'All folders' : s.folder || 'Unknown folder';
  const waiting = useAfter(s.inventoryRead.kind === 'loading' || s.sessionsRead.kind === 'loading');
  // Where the transcript had the focus, the list takes it back when the person closes it.
  useEffect(() => {
    c.focusList = () => document.querySelector<HTMLElement>('[role="tree"][aria-label="Sessions"] [role="treeitem"][aria-selected="true"], [role="tree"][aria-label="Sessions"] [role="treeitem"]')?.focus();
    return () => { c.focusList = () => {}; };
  }, [c]);

  let body;
  if (!installation) {
    if (s.inventoryRead.kind === 'error') body = <Empty fill title="The stores could not be read" description={<Note tone="error" title={s.inventoryRead.message} actions={<Button variant="outline" onClick={() => void c.loadInventory()}>Reload stores</Button>} />} />;
    else if (s.inventoryRead.kind === 'loading') body = waiting ? <Empty fill title="Reading the stores" action={<Progress inline aria-label="Reading the stores" value={null} />} /> : null;
    else body = <Empty fill title="No installations found" description="Scan a folder for portable or custom installations." action={<Button variant="outline" onClick={() => void c.scanFolder()}>Scan folder</Button>} />;
  } else if (installation.error) {
    body = <Empty fill title={installation.label} description={<Note tone="error" title={installation.error} actions={<Button variant="outline" onClick={() => void c.loadInventory()}>Read the stores again</Button>} />} />;
  } else if (s.sessionsRead.kind === 'error') {
    body = <Empty fill title="The sessions could not be read" description={<Note tone="error" title={s.sessionsRead.message} actions={<Button variant="outline" onClick={() => void c.pickInstallation(installation.key, true)}>Retry</Button>} />} />;
  } else if (s.sessionsRead.kind === 'loading') {
    body = waiting ? <Empty fill title="Reading the sessions" action={<Progress inline aria-label="Reading the sessions" value={null} />} /> : null;
  } else if (!nodes.length) {
    body = <Empty fill title={searching ? `No session matches “${s.query.trim()}”` : `${installation.label} has no sessions${s.folder !== null ? ' in this folder' : ''}`} />;
  } else {
    body = (
      <Tree
        aria-label="Sessions"
        fill
        virtual
        followFocus
        typeahead={false}
        nodes={nodes}
        selected={s.selected}
        onSelect={(id) => c.pickSession(id)}
        onOpen={(id) => { c.pickSession(id); void c.openReader(); }}
        expanded={open}
        onExpandedChange={(next) => { if (!searching) c.setCollapsed(foldedAfter(nested, next, s.collapsed)); }}
        onKeyDown={(event, id) => {
          if (event.ctrlKey || event.metaKey || event.altKey || event.shiftKey) return;
          const step = event.key === 'j' ? 1 : event.key === 'k' ? -1 : 0;
          if (step === 0) return;
          event.preventDefault();
          const ids = shownIds(nested, new Set(open));
          const next = ids[Math.max(0, Math.min(ids.length - 1, ids.indexOf(id) + step))];
          if (next !== undefined) c.pickSession(next);
        }}
      />
    );
  }

  return (
    <Panel
      title="Sessions"
      subtitle={<>{folder} · {searching ? `${count} match` : count}</>}
      tools={(
        <SearchField
          ref={field}
          aria-label="Filter sessions"
          placeholder="Filter by title, id, folder"
          value={s.query}
          onValueChange={(value) => c.setQuery(value)}
          onKeyDown={(event) => { if (event.key === 'Enter') { event.preventDefault(); (document.querySelector('[data-slot="tree-rows"] [role="treeitem"][tabindex="0"]') as HTMLElement | null)?.focus(); } }}
        />
      )}
    >
      {body}
    </Panel>
  );
}

/** The sessions view: the list, and beside it the transcript where there is room (one at a time where there is not). */
export function SessionsScreen({ field }: { field: Ref<HTMLInputElement> }) {
  const c = useController();
  const reading = c.state.reader !== null;
  const [panes, setPanes] = useState<1 | 2>(2);
  return (
    <SplitPane
      start={<SessionList field={field} />}
      startLabel="Sessions of the installation"
      endLabel="Transcript"
      label="Resize the sessions list"
      storageKey={SPLIT_KEY}
      defaultSize={420}
      minStart={240}
      minEnd={320}
      show={reading ? 'end' : 'start'}
      onPanesChange={setPanes}
    >
      <Reader single={panes === 1} />
    </SplitPane>
  );
}

