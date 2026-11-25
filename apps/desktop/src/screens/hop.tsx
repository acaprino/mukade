// F6, F8: the hop dialog (destination, compaction and its own options, subagents, branches, reasoning, shell, prompt, model and tools, and
// the plan before anything runs), the import dialog, and the receipt both end in. One dialog for the three: a hop and an import are a
// write that Rust owns until the person dismisses its result (write-recovery.ts), so the dialog is a form, then the wait, then the
// receipt, or the failure. A window that was closed while a write ran opens it again on the wait (`recover`).
import { Fragment, useEffect, useMemo, useRef, useState, type FormEvent } from 'react';
import { Dialog, DescriptionList, Disclosure, FormField, Note, CopyButton } from '@acaprino/sigrid/components';
import { Button, Cell, Checkbox, Cluster, Column, Grid, Icon, Input, Progress, RadioGroup, RadioGroupItem, Section, Select, Spacer, Text, TextList, Textarea, ToggleGroup, ToggleGroupItem } from '@acaprino/sigrid/primitives';
import { api, errorText } from '../api.ts';
import { commandLine } from '../chatmodel.ts';
import type { Controller } from '../controller.ts';
import { WriteWatch, type Terminal } from '../write-recovery.ts';
import type { HopArgs, HopOptions, Installation, Receipt, WriteOperation } from '../types.ts';

const COMPACTION_HELP: Record<string, string> = {
  none: 'the whole history',
  portable: 'the source’s own last summary, then what follows it',
  summary: 'the source’s model writes a summary; the history travels as it',
  native: 'the whole history, then the destination’s own compaction record',
};
const SHELLS = ['default', 'posix', 'powershell', 'bash', 'pwsh', 'none'];
/** Whether the hop dialog's More options are open: kept while the window lives. */
let moreOpen = false;

type Phase =
  | { kind: 'form' }
  | { kind: 'running'; label: string }
  | { kind: 'done'; receipt: Receipt; what: WriteOperation['kind']; subtitle: string }
  | { kind: 'failed'; reason: string; what: WriteOperation['kind'] };

/**
 * Enter on a check box or a radio is the control's own and starts nothing: a hop is a write, and only its own button, or Enter in a
 * one-line field, begins it. Base UI gives Enter on those controls the form's submission; this takes it away in the capture phase.
 */
function holdEnter(event: React.KeyboardEvent<HTMLFormElement>): void {
  const target = event.target;
  if (event.key !== 'Enter' || !(target instanceof HTMLElement)) return;
  if (['checkbox', 'radio', 'switch'].includes(target.getAttribute('role') ?? '')) { event.preventDefault(); event.stopPropagation(); }
}

/** A whole number of at least 1, or nothing. */
const count = (text: string): number | undefined => { const n = Number(text.trim()); return text.trim() && Number.isInteger(n) && n >= 1 ? n : undefined; };
const text = (value?: string) => value?.trim() || undefined;

function Group({ label, children }: { label: string; children: React.ReactNode }) {
  return <Column gap="xs"><Text variant="strong">{label}</Text>{children}</Column>;
}

/** A command, as a line to copy. */
function Command({ label, argv }: { label: string; argv: string[] }) {
  const line = commandLine(argv);
  return (
    <Group label={label}>
      <Cluster gap="sm" wrap={false}>
        <Cell grow><Text as="div" mono pre>{line}</Text></Cell>
        <CopyButton value={line} variant="outline" size="sm" />
      </Cluster>
    </Group>
  );
}

/** The receipt of a hop or import (spec F6 result). */
function ReceiptView({ receipt }: { receipt: Receipt }) {
  return (
    <Grid min="md" gap="lg">
      <Column gap="md">
        {receipt.warning ? <Note tone="warning" title={receipt.warning} /> : null}
        <DescriptionList
          aria-label="What was written"
          items={[
            { name: 'New id', value: receipt.dest_id, code: true },
            { name: 'Into', value: receipt.dest_label },
            { name: 'Location', value: receipt.location, code: true },
            { name: 'Manifest', value: receipt.manifest_path ?? 'not saved (no mukade-data folder)', code: Boolean(receipt.manifest_path) },
          ]}
        />
        {receipt.refusal ? <Group label="Resume · interactive"><Note tone="info" title={receipt.refusal} /></Group> : <Command label="Resume · interactive" argv={receipt.interactive} />}
        {receipt.headless.length ? <Command label="Resume · headless" argv={receipt.headless} /> : null}
        {receipt.children.length ? (
          <Group label={`Subagents written · ${receipt.children.length}`}>
            <TextList items={receipt.children.map((child) => <Text key={child.dest_id}>↳ {child.source_id} → {child.dest_id}{child.linked ? <Text variant="muted"> · linked to its parent</Text> : <Text tone="warning"> · not linked: the destination has no link</Text>}</Text>)} />
          </Group>
        ) : null}
      </Column>
      <Column gap="md">
        <Group label={receipt.losses.length ? `Losses · ${receipt.losses.length}` : 'Losses'}>
          {receipt.losses.length ? <TextList items={receipt.losses} /> : <Note tone="success" title="None: everything travelled." />}
        </Group>
        {receipt.reply ? <Group label={`Reply from ${receipt.dest_label}`}><Text as="div" mono pre>{receipt.reply}</Text></Group> : null}
      </Column>
    </Grid>
  );
}

/** The hop's form, and the plan beside it: what will be written, before anything is. */
function HopForm({ options, choice, setChoice, formId, onSubmit }: {
  options: HopOptions; choice: HopArgs; setChoice: (next: HopArgs) => void; formId: string; onSubmit: () => void;
}) {
  const dest = options.destinations.find((d) => d.key === choice.dest)!;
  const [more, setMore] = useState(moreOpen);
  const set = (patch: Partial<HopArgs>) => setChoice({ ...choice, ...patch });
  const summary = choice.compaction === 'summary';
  const native = choice.compaction === 'native';
  const compactions = options.compactions.filter((c) => !(choice.all_branches && c === 'summary'));
  const notTravelling = [
    options.forks.length ? `${options.forks.length === 1 ? 'the fork' : `${options.forks.length} forks`} (${options.forks.join(', ')}): hop each on its own` : '',
    !choice.children && options.subagents.length ? `the ${options.subagents.length} subagent session${options.subagents.length === 1 ? '' : 's'}` : '',
  ].filter(Boolean);
  const plan = [
    <Text key="write"><Text variant="strong">write</Text> 1 session into {dest.label}</Text>,
    choice.children && options.subagents.length ? <Text key="subs"><Text variant="strong">write</Text> {options.subagents.length} subagent session{options.subagents.length === 1 ? '' : 's'} with it{options.subagents.map((name) => ` · ↳ ${name}`).join('')}</Text> : null,
    <Text key="history"><Text variant="strong">history</Text> {COMPACTION_HELP[choice.compaction] ?? choice.compaction}</Text>,
    summary ? <Text key="summary"><Text variant="strong">summary</Text> intent {choice.intent ?? options.intents[0]}, by {choice.summary_model?.trim() || 'the session’s own model'}{choice.keep_turns ? `, then the last ${choice.keep_turns} turn${choice.keep_turns === 1 ? '' : 's'} whole` : ''}</Text> : null,
    native ? <Text key="window" tone={choice.context_window ? undefined : 'warning'}><Text variant="strong">window</Text> {choice.context_window ? `${choice.context_window} tokens${choice.max_output_tokens ? `, ${choice.max_output_tokens} of output` : ''}` : 'the context window is needed'}</Text> : null,
    choice.all_branches ? <Text key="branches"><Text variant="strong">branches</Text> every branch the source keeps</Text> : null,
    <Text key="reasoning"><Text variant="strong">reasoning</Text> {choice.reasoning}</Text>,
    <Text key="shell"><Text variant="strong">shell</Text> {choice.dest_shell === 'default' ? `tool calls for ${dest.default_shell ?? 'the destination’s own shell'}` : choice.dest_shell === 'none' ? 'commands left as written' : `tool calls for ${choice.dest_shell}`}</Text>,
    choice.model?.trim() || choice.model_provider?.trim() ? <Text key="model"><Text variant="strong">model</Text> {[choice.model_provider?.trim(), choice.model?.trim()].filter(Boolean).join(' · ')}</Text> : null,
    choice.without_tools ? <Text key="tools"><Text variant="strong">tools</Text> left out: the conversation alone</Text> : null,
    choice.interrupted === 'drop' ? <Text key="interrupted"><Text variant="strong">interrupted</Text> the work of unfinished turns is left out</Text> : null,
    choice.prompt?.trim() ? <Text key="then"><Text variant="strong">then</Text> run the destination once: “{choice.prompt.trim()}”</Text> : null,
    <Text key="save"><Text variant="strong">save</Text> the manifest in mukade-data/manifests</Text>,
  ].filter(Boolean);
  return (
    <form id={formId} onKeyDownCapture={holdEnter} onSubmit={(event: FormEvent) => { event.preventDefault(); onSubmit(); }}>
      <Grid min="md" gap="lg">
        <Column gap="md">
          <Group label="Destination">
            <RadioGroup value={choice.dest} onValueChange={(dest) => set({ dest })} aria-label="Destination">
              {options.destinations.map((item) => (
                <RadioGroupItem key={item.key} value={item.key}>{item.label}{item.default_shell ? <Text variant="muted"> · {item.default_shell}</Text> : null}</RadioGroupItem>
              ))}
            </RadioGroup>
          </Group>
          <Group label="Compaction">
            <ToggleGroup aria-label="Compaction" value={choice.compaction} onValueChange={(compaction) => set({ compaction })}>
              {compactions.map((name) => <ToggleGroupItem key={name} value={name}>{name}</ToggleGroupItem>)}
            </ToggleGroup>
            <Text variant="muted">{COMPACTION_HELP[choice.compaction] ?? ''}</Text>
          </Group>
          {summary ? (
            <Cluster gap="md" align="start">
              <FormField label="Intent"><Select options={options.intents.map((name) => ({ value: name, label: name }))} value={choice.intent ?? options.intents[0] ?? ''} onValueChange={(intent) => set({ intent })} /></FormField>
              <FormField label="Summary model"><Input value={choice.summary_model ?? ''} placeholder="the session’s own" autoComplete="off" onChange={(e) => set({ summary_model: e.target.value })} /></FormField>
              <FormField label="Turns kept whole"><Input short="xs" value={choice.keep_turns ? String(choice.keep_turns) : ''} placeholder="0" autoComplete="off" onChange={(e) => set({ keep_turns: count(e.target.value) })} /></FormField>
            </Cluster>
          ) : null}
          {native ? (
            <Cluster gap="md" align="start">
              <FormField label="Context window · tokens" help="of the destination’s model, e.g. 200000"><Input short="sm" value={choice.context_window ? String(choice.context_window) : ''} autoComplete="off" onChange={(e) => set({ context_window: count(e.target.value) })} /></FormField>
              <FormField label="Output limit · optional"><Input short="sm" value={choice.max_output_tokens ? String(choice.max_output_tokens) : ''} placeholder="tokens" autoComplete="off" onChange={(e) => set({ max_output_tokens: count(e.target.value) })} /></FormField>
            </Cluster>
          ) : null}
          {options.subagents.length ? (
            <Group label={`Subagents · ${options.subagents.length} found`}>
              <ToggleGroup aria-label="Subagents" value={choice.children ? 'carry' : 'leave'} onValueChange={(value) => set({ children: value === 'carry' })}>
                <ToggleGroupItem value="carry">carry</ToggleGroupItem>
                <ToggleGroupItem value="leave">leave</ToggleGroupItem>
              </ToggleGroup>
            </Group>
          ) : null}
          <Group label="Reasoning">
            <ToggleGroup aria-label="Reasoning" value={choice.reasoning} onValueChange={(reasoning) => set({ reasoning })}>
              {['drop_foreign', 'as_text', 'drop'].map((name) => <ToggleGroupItem key={name} value={name}>{name}</ToggleGroupItem>)}
            </ToggleGroup>
          </Group>
          <Cluster gap="md" align="start">
            <FormField label="Destination shell">
              <Select
                options={SHELLS.map((name) => ({ value: name, label: name === 'default' ? `default · ${dest.default_shell ?? 'none known'}` : name }))}
                value={choice.dest_shell}
                onValueChange={(dest_shell) => set({ dest_shell })}
              />
            </FormField>
            <Checkbox
              label={options.all_branches_supported ? 'All branches' : 'All branches · not kept by this source'}
              checked={choice.all_branches}
              disabled={!options.all_branches_supported}
              onCheckedChange={(on) => set({ all_branches: on === true, ...(on === true && choice.compaction === 'summary' ? { compaction: 'none' } : {}) })}
            />
          </Cluster>
          <FormField label="Prompt · optional, runs the destination once">
            <Textarea mono={false} rows={2} placeholder="continue" value={choice.prompt ?? ''} onChange={(e) => set({ prompt: e.target.value })} />
          </FormField>
          <Disclosure label="More options" open={more} onOpenChange={(open) => { moreOpen = open; setMore(open); }}>
            <Column gap="md">
              <Cluster gap="md" align="start">
                <FormField label="Destination model"><Input value={choice.model ?? ''} placeholder={dest.model ?? 'the destination’s own'} autoComplete="off" onChange={(e) => set({ model: e.target.value })} /></FormField>
                <FormField label="Provider"><Input value={choice.model_provider ?? ''} placeholder="the destination’s own" autoComplete="off" onChange={(e) => set({ model_provider: e.target.value })} /></FormField>
              </Cluster>
              <Group label="Unfinished turns">
                <ToggleGroup aria-label="Unfinished turns" value={choice.interrupted ?? 'keep'} onValueChange={(value) => set({ interrupted: value as 'keep' | 'drop' })}>
                  <ToggleGroupItem value="keep">keep</ToggleGroupItem>
                  <ToggleGroupItem value="drop">drop</ToggleGroupItem>
                </ToggleGroup>
              </Group>
              <Checkbox label="Without tool calls and results" checked={Boolean(choice.without_tools)} onCheckedChange={(on) => set({ without_tools: on === true })} />
            </Column>
          </Disclosure>
        </Column>
        <Column gap="md">
          <Section title="Plan · nothing is written yet" level={3}><TextList items={plan} /></Section>
          {notTravelling.length ? <Note tone="warning" title={`Will not travel: ${notTravelling.join('; ')}.`} /> : null}
          <Note tone="info" title="What the destination cannot hold is listed as a loss in the receipt; no loss is silent." />
          {options.live ? <Note tone="info" title="The source session is live. The hop copies what is on disk now." /> : null}
        </Column>
      </Grid>
    </form>
  );
}

/** The import's form: a document, the installation it goes into, and a prompt to run once. */
function ImportForm({ writable, document, setDocument, into, setInto, prompt, setPrompt, formId, onSubmit, pick }: {
  writable: Installation[]; document: string; setDocument: (value: string) => void; into: string; setInto: (value: string) => void;
  prompt: string; setPrompt: (value: string) => void; formId: string; onSubmit: () => void; pick: () => void;
}) {
  return (
    <form id={formId} onKeyDownCapture={holdEnter} onSubmit={(event: FormEvent) => { event.preventDefault(); onSubmit(); }}>
      <Column gap="md">
        <FormField label="Document">
          <Input mono value={document} placeholder="C:\…\mukade-data\exports\<harness>-<id>.json" autoComplete="off" spellCheck={false} onChange={(e) => setDocument(e.target.value)} />
        </FormField>
        <Cluster><Button variant="outline" size="sm" onClick={pick}><Icon name="folder" />Choose…</Button></Cluster>
        <FormField label="Into"><Select options={writable.map((item) => ({ value: item.key, label: item.label }))} value={into} onValueChange={setInto} /></FormField>
        <FormField label="Prompt · optional, runs the destination once"><Textarea rows={2} placeholder="continue" value={prompt} onChange={(e) => setPrompt(e.target.value)} /></FormField>
        <Note tone="info" title="The document becomes a new session of the installation, with its own id; the source is not touched." />
      </Column>
    </form>
  );
}

export function WriteDialog({ c, kind, hop, importing }: {
  c: Controller;
  kind: 'hop' | 'import' | 'recover';
  hop?: { key: string; id: string };
  importing?: { document: string | null; into: string | null };
}) {
  const formId = 'write-form';
  const watch = useRef(new WriteWatch((message) => c.report(message, true))).current;
  const [phase, setPhase] = useState<Phase>(kind === 'recover' ? { kind: 'running', label: 'In progress…' } : { kind: 'form' });
  const [options, setOptions] = useState<HopOptions | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [choice, setChoice] = useState<HopArgs | null>(null);
  const writable = useMemo(() => c.state.installations.filter((item) => item.destination), [c]);
  const [document, setDocument] = useState(importing?.document ?? '');
  const [into, setInto] = useState(importing?.into && writable.some((item) => item.key === importing.into) ? importing.into : writable[0]?.key ?? '');
  const [prompt, setPrompt] = useState('');
  const [recovered, setRecovered] = useState<'hop' | 'import' | null>(null);

  // The result is dismissed (acknowledged) when the dialog goes, whatever way it goes.
  useEffect(() => () => { void watch.acknowledge(); }, [watch]);

  useEffect(() => {
    if (kind === 'recover') {
      api.writeStatus().then((current) => {
        if (!current) { c.setJob(null); c.close(); return; }
        setRecovered(current.kind);
        void follow(current, undefined, current.kind);
      }, (error) => { setPhase({ kind: 'failed', reason: errorText(error), what: 'hop' }); c.setJob(null); });
      return;
    }
    if (kind !== 'hop' || !hop) return;
    let live = true;
    api.hopOptions(hop.key, hop.id).then((got) => {
      if (!live) return;
      setOptions(got);
      if (got.destinations.length) {
        setChoice({
          source: hop.key, id: hop.id, dest: got.destinations[0]!.key, compaction: 'none', children: got.subagents.length > 0, all_branches: false,
          reasoning: 'drop_foreign', dest_shell: 'default', prompt: '', interrupted: 'keep', without_tools: false,
        });
      }
    }, (error) => { if (live) setLoadError(errorText(error)); });
    return () => { live = false; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  /** Follows a write to its end and shows its receipt or its failure. */
  async function follow(start: WriteOperation | (() => Promise<WriteOperation>), source: string | undefined, what: WriteOperation['kind']): Promise<void> {
    try {
      const result: Terminal = await watch.observe(start);
      if (result.status === 'failed') throw result.error;
      const receipt = result.receipt;
      c.report(receipt.warning ? `${result.kind} written, with a warning: ${receipt.warning}` : `${result.kind} done → ${receipt.dest_label} · ${receipt.dest_id}`, Boolean(receipt.warning));
      setPhase({ kind: 'done', receipt, what: result.kind, subtitle: `${source ?? (result.kind === 'import' ? 'document' : 'session')} → ${receipt.dest_label}` });
    } catch (error) {
      const reason = errorText(error);
      c.report(`${what === 'import' ? 'Import' : 'Hop'} failed: ${reason}`, true);
      setPhase({ kind: 'failed', reason, what });
    } finally {
      c.setJob(null);
    }
  }

  const runHop = () => {
    if (!options || !choice) return;
    const dest = options.destinations.find((d) => d.key === choice.dest)!;
    const summary = choice.compaction === 'summary';
    const native = choice.compaction === 'native';
    if (native && !choice.context_window) return;
    setPhase({ kind: 'running', label: `Hopping to ${dest.label}…` });
    c.setJob(`Hopping → ${dest.label}…`);
    void follow(() => api.hop({
      ...choice, prompt: text(choice.prompt), model: text(choice.model), model_provider: text(choice.model_provider),
      intent: summary ? (choice.intent ?? options.intents[0]) : undefined, summary_model: summary ? text(choice.summary_model) : undefined, keep_turns: summary ? choice.keep_turns : undefined,
      context_window: native ? choice.context_window : undefined, max_output_tokens: native ? choice.max_output_tokens : undefined,
    }), options.source_label, 'hop');
  };

  const runImport = () => {
    if (!document.trim()) return;
    const target = writable.find((item) => item.key === into);
    setPhase({ kind: 'running', label: `Importing into ${target?.label}…` });
    c.setJob(`Importing → ${target?.label}…`);
    void follow(() => api.import(document.trim(), into, prompt.trim() || undefined), 'document', 'import');
  };

  const running = phase.kind === 'running';
  const close = () => { if (!running) c.close(); };
  const what = recovered ?? kind;
  const title = phase.kind === 'done' ? (phase.receipt.warning ? 'Written, with a warning' : 'Done') : what === 'import' ? 'Import' : 'Hop';
  const description = phase.kind === 'done' ? phase.subtitle
    : what === 'import' ? 'a dump document into an installation'
      : options ? `${options.source_label} · ${options.title} · ${hop?.id.slice(0, 8)}` : undefined;
  const dest = options && choice ? options.destinations.find((d) => d.key === choice.dest) : undefined;
  // Each phase has footer buttons of its own: a button that turns into the form's submit while the press that made it is still being handled would submit it.

  let body;
  let footer;
  if (phase.kind === 'running') {
    body = <Column gap="md"><Text>{phase.label}</Text><Progress aria-label={phase.label} value={null} /></Column>;
    footer = <Text variant="muted">A hop cannot be stopped once it starts.</Text>;
  } else if (phase.kind === 'done') {
    const receipt = phase.receipt;
    body = <ReceiptView receipt={receipt} />;
    footer = (
      <>
        {receipt.manifest_path ? <CopyButton value={receipt.manifest_path} label="Copy the manifest path" showLabel variant="outline" size="sm" /> : null}
        <Spacer />
        <Button variant="outline" onClick={() => { c.close(); void c.revealSession(receipt.dest_key, receipt.dest_id, true); }}>Show in sessions</Button>
        {receipt.refusal ? null : <Button variant="outline" onClick={() => { c.close(); void c.resume(receipt.dest_key, receipt.dest_id); }}>Resume in terminal</Button>}
        <Button variant="default" autoFocus onClick={() => c.close()}>Close</Button>
      </>
    );
  } else if (phase.kind === 'failed') {
    body = <Note tone="error" title={`${phase.what === 'import' ? 'Import' : 'Hop'} failed`}>{phase.reason}</Note>;
    footer = (
      <>
        <Spacer />
        <Button variant="outline" onClick={() => c.close()}>Close</Button>
        {phase.what === 'import' && kind === 'import' ? <Button variant="default" onClick={() => { void watch.acknowledge(); setPhase({ kind: 'form' }); }}>Try again</Button> : null}
      </>
    );
  } else if (kind === 'import') {
    body = (
      <ImportForm
        writable={writable} document={document} setDocument={setDocument} into={into} setInto={setInto} prompt={prompt} setPrompt={setPrompt} formId={formId} onSubmit={runImport}
        pick={() => { void api.pick('open').then((path) => { if (path) setDocument(path); }, (error) => c.report(errorText(error), true)); }}
      />
    );
    footer = (
      <>
        <Spacer />
        <Button variant="outline" onClick={close}>Cancel</Button>
        <Button type="submit" form={formId} variant="default" disabled={!document.trim() || !into}>Import</Button>
      </>
    );
  } else if (loadError) {
    body = <Note tone="error" title={loadError} />;
    footer = <><Spacer /><Button variant="outline" onClick={close}>Close</Button></>;
  } else if (!options || (options.destinations.length > 0 && !choice)) {
    body = <Progress aria-label="Looking at where it can go" value={null} />;
    footer = <><Spacer /><Button variant="outline" onClick={close}>Cancel</Button></>;
  } else if (!options.destinations.length || !choice) {
    body = <Note tone="warning" title="No other installation mukade can write into." />;
    footer = <><Spacer /><Button variant="outline" onClick={close}>Close</Button></>;
  } else {
    body = <HopForm options={options} choice={choice} setChoice={setChoice} formId={formId} onSubmit={runHop} />;
    footer = (
      <>
        <Text variant="muted">A hop cannot be stopped once it starts.</Text>
        <Spacer />
        <Button variant="outline" onClick={close}>Cancel</Button>
        <Button type="submit" form={formId} variant="default" disabled={choice.compaction === 'native' && !choice.context_window}>Hop to {dest?.label}</Button>
      </>
    );
  }

  return (
    <Dialog open onOpenChange={(open) => { if (!open) close(); }} title={title} description={description} width="lg" footer={<Fragment key={phase.kind}>{footer}</Fragment>}>
      {body}
    </Dialog>
  );
}
