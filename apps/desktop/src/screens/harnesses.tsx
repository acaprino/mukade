// The harnesses page: a card for every harness with its binary, version and standard store; under it the other installations found for
// it (portable ones, sandboxes); a scan of the whole computer, one folder at a time; installs: the harness's own command in a terminal,
// or a sandbox the window installs with npm, with a store of its own. It has no sidebar: the page is the cards.
import { Empty, DescriptionList, ItemList, ItemRow, Note } from '@acaprino/sigrid/components';
import { Panel } from '@acaprino/sigrid/layout';
import {
  Badge, Button, Card, Cluster, Column, DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger, Grid, Heading, Icon, Progress, Section, Split, Text,
} from '@acaprino/sigrid/primitives';
import type { Controller } from '../controller.ts';
import type { HarnessView, Installation, Sandbox } from '../types.ts';
import { useController, useStore } from './hooks.ts';
import { HarnessMark } from './mark.tsx';

const STATUS_TEXT: Record<HarnessView['status'], string> = { installed: 'installed', configured: 'configured', missing: 'not found' };

/** The installations of a harness besides the standard one: the ones a scan found and the sandboxes the window made. */
function More({ c, others, sandboxes }: { c: Controller; others: Installation[]; sandboxes: Sandbox[] }) {
  const loose = sandboxes.filter((one) => !others.some((item) => item.store === one.store));
  return (
    <Section title={`More installations · ${others.length + loose.length}`} level={4} icon={<Icon name="layers" />}>
      <ItemList aria-label="More installations">
        {others.map((item) => {
          const sandbox = sandboxes.find((one) => one.store === item.store);
          return (
            <ItemRow
              key={item.key}
              title={item.label}
              description={item.engine ? `${item.store ?? ''} · engine ${item.engine}` : item.store}
              meta={item.error ? 'error' : `${item.count} sessions`}
              end={sandbox ? <Badge variant="secondary">sandbox</Badge> : undefined}
              disabled={item.count === 0}
              onClick={() => { c.chooseView('sessions'); void c.pickInstallation(item.key); }}
              control={sandbox ? <SandboxMenu c={c} sandbox={sandbox} /> : undefined}
              controlSide="end"
            />
          );
        })}
        {loose.map((sandbox) => (
          <ItemRow
            key={sandbox.root}
            title={`sandbox ${sandbox.name}`}
            description={`${sandbox.root} · its store appears at the first run`}
            end={<Badge variant="secondary">sandbox</Badge>}
            disabled
            control={<SandboxMenu c={c} sandbox={sandbox} />}
            controlSide="end"
          />
        ))}
      </ItemList>
    </Section>
  );
}

function SandboxMenu({ c, sandbox }: { c: Controller; sandbox: Sandbox }) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" size="icon-sm" aria-label={`Sandbox ${sandbox.name}`} title={`Sandbox ${sandbox.name}`}><Icon name="more" /></Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuItem onSelect={() => void c.harnesses.runSandbox(sandbox)}>Run in a terminal</DropdownMenuItem>
        <DropdownMenuItem onSelect={() => void c.harnesses.forgetSandbox(sandbox)}>Forget sandbox</DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function HarnessCard({ c, harness }: { c: Controller; harness: HarnessView }) {
  const h = c.harnesses;
  const installations = c.state.installations;
  const standard = installations.find((item) => item.key === harness.id);
  const others = installations.filter((item) => item.harness === harness.id && item.scanned);
  const sandboxes = h.sandboxes.filter((one) => one.harness === harness.id);
  const installing = h.installing.has(harness.id);
  return (
    <Card as="section" aria-label={harness.display}>
      <Split>
        <Cluster gap="sm">
          <HarnessMark harness={harness.id} size="md" />
          <Heading size="card" level={3}>{harness.display}</Heading>
          <Badge tone={harness.status === 'missing' ? 'danger' : 'success'}>{STATUS_TEXT[harness.status]}</Badge>
          {harness.version ? <Text mono variant="muted">{harness.version}</Text> : null}
        </Cluster>
        <Button variant="outline" size="sm" disabled={installing} onClick={() => c.open({ kind: 'install', harness })}><Icon name="download" />{installing ? 'Installing…' : 'Install…'}</Button>
      </Split>
      <DescriptionList
        aria-label={`${harness.display}: binary and store`}
        items={[
          { name: 'Binary', value: harness.binary ?? `not on PATH and not at its usual place; name one with ${harness.binary_env}`, code: Boolean(harness.binary) },
          harness.binary ? { name: 'Found by', value: harness.source } : null,
          { name: 'Store', value: <>{harness.store ?? 'none known'}{harness.store && !harness.store_exists ? <Text variant="muted"> · not there yet</Text> : null}</>, code: Boolean(harness.store) },
          { name: 'Sessions', value: standard?.error ? <Text tone="danger">{standard.error}</Text> : String(standard?.count ?? 0) },
          { name: 'Writes', value: harness.destination ? 'yes: a hop or an import can land here' : 'no: read only' },
        ]}
      />
      {standard && standard.count > 0 ? (
        <Cluster><Button variant="outline" size="sm" onClick={() => { c.chooseView('sessions'); void c.pickInstallation(standard.key); }}><Icon name="history" />Show sessions</Button></Cluster>
      ) : null}
      {others.length || sandboxes.length ? <More c={c} others={others} sandboxes={sandboxes} /> : null}
    </Card>
  );
}

export function HarnessesScreen() {
  const c = useController();
  const h = c.harnesses;
  useStore(h);
  const found = c.state.installations.filter((item) => item.scanned).length;
  const scan = h.scan;
  const folders = c.state.scan;
  let body;
  if (h.error) {
    body = <Empty fill title="The harnesses could not be checked" description={<Note tone="error" title={h.error} actions={<Button variant="outline" onClick={() => void h.refresh()}>Check again</Button>} />} />;
  } else if (!h.inventory) {
    body = <Empty fill title="Looking for the harnesses" action={<Progress inline aria-label="Looking for the harnesses" value={null} />} />;
  } else {
    body = (
      <Column gap="lg" pad="lg">
        <Grid min="lg" gap="lg">
          {h.inventory.map((harness) => <HarnessCard key={harness.id} c={c} harness={harness} />)}
        </Grid>
        <Section title={`Folders and stores scanned · ${folders.length}`} level={3} icon={<Icon name="folder" />}>
          {folders.length ? (
            <Column gap="xs">
              {folders.map((dir) => (
                <Split key={dir}>
                  <Text mono truncate>{dir}</Text>
                  <Button variant="ghost" size="sm" title="Stop listing what this folder holds" onClick={() => void h.forgetFolder(dir)}>Forget folder</Button>
                </Split>
              ))}
            </Column>
          ) : <Text variant="muted">Scan this computer, or one folder, to find portable and custom installations.</Text>}
        </Section>
      </Column>
    );
  }
  return (
    <Panel
      title="Harnesses"
      subtitle={h.inventory ? `${h.inventory.filter((one) => one.status !== 'missing').length} of ${h.inventory.length} found · ${found} more installation${found === 1 ? '' : 's'}` : h.reading ? 'looking…' : undefined}
      tools={(
        <Column gap="sm">
        <Cluster gap="sm">
          {scan
            ? <Button variant="outline" size="sm" onClick={() => h.stopScan()}>Stop the scan</Button>
            : <Button variant="default" size="sm" title="Look in your home and in every folder of every drive" onClick={() => void h.scanComputer()}><Icon name="search" />Scan this computer</Button>}
          <Button variant="outline" size="sm" onClick={() => void c.scanFolder()}><Icon name="folder" />Scan folder…</Button>
          <Button variant="outline" size="sm" disabled={h.reading} onClick={() => void h.refresh()}><Icon name="refresh" />Check again</Button>
        </Cluster>
        {scan ? (
          <Cluster gap="sm">
            <Text truncate>Scanning {scan.root} · {scan.done} of {scan.total} folders · {scan.found} found</Text>
            <Progress inline size="sm" aria-label="System scan" value={scan.total ? Math.round((scan.done / scan.total) * 100) : null} showValue />
          </Cluster>
        ) : null}
        </Column>
      )}
    >
      {body}
    </Panel>
  );
}
