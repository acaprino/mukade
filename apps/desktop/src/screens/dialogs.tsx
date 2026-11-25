// The dialogs of the window, each Sigrid's `Dialog` (or `ConfirmDialog`), drawn from `Controller.dialog` while it is open: what exporting
// says, why a session was not resumed, starting another harness in a session's folder, the keys, the settings, a new room, an agent
// launched into a room, the install of a harness, and the confirmations. The hop, the import and their receipt are hop.tsx; the
// search of everything is search.tsx. The focus goes to the first control, and Escape closes: Sigrid's.
import { useEffect, useState, type FormEvent } from 'react';
import { useGlass, useWindow } from '@acaprino/imago';
import { useCommandPalette, useCommands } from '@acaprino/sigrid/assemblies';
import { ConfirmDialog, CopyButton, Dialog, DescriptionList, FormField, Note, SettingRow } from '@acaprino/sigrid/components';
import { Button, Cell, Cluster, Column, Icon, Input, Progress, Section, Select, Spacer, Switch, Text, Textarea, ToggleGroup, ToggleGroupItem } from '@acaprino/sigrid/primitives';
import { api, errorText } from '../api.ts';
import { HARNESSES, ROLES } from '../chatcontroller.ts';
import { commandLine, roomNameRefusal, roomNamed } from '../chatmodel.ts';
import type { Controller, Dialog as DialogState } from '../controller.ts';
import type { HandoffOptions, HarnessView, Resumed, SessionRow } from '../types.ts';
import { WriteDialog } from './hop.tsx';
import { SearchDialog } from './search.tsx';

const CANCEL = (c: Controller) => <Button variant="outline" onClick={() => c.close()}>Cancel</Button>;

function Exported({ c, exported }: { c: Controller; exported: { path: string; messages: number } }) {
  return (
    <Dialog
      open
      onOpenChange={(open) => { if (!open) c.close(); }}
      title="Exported"
      description={`${exported.messages} messages`}
      footer={(
        <>
          <Spacer />
          <Button variant="outline" onClick={() => { c.close(); c.importDocument(exported.path); }}><Icon name="upload" />Import it…</Button>
          <Button variant="default" onClick={() => c.close()}>Close</Button>
        </>
      )}
    >
      <Column gap="md">
        <DescriptionList aria-label="The document" items={[{ name: 'Document', value: exported.path, code: true }, { name: 'Messages', value: String(exported.messages) }]} />
        <Note tone="info" title="Import it into another installation with Import, or from a terminal.">
          <Text mono>{`mukade write <harness> "${exported.path}"`}</Text>
        </Note>
      </Column>
    </Dialog>
  );
}

function ResumeRefused({ c, result }: { c: Controller; result: Resumed }) {
  const command = commandLine(result.command);
  return (
    <Dialog
      open
      onOpenChange={(open) => { if (!open) c.close(); }}
      title="Resume"
      description="this harness has no terminal UI"
      footer={(
        <>
          <Spacer />
          <Button variant="outline" onClick={() => c.close()}>Close</Button>
          <CopyButton value={command || result.refusal || ''} label="Copy the command" showLabel variant="default" />
        </>
      )}
    >
      <Column gap="md">
        <Note tone="info" title={result.refusal ?? ''} />
        {command ? <Text as="div" mono pre>{command}</Text> : null}
      </Column>
    </Dialog>
  );
}

/** Start another harness in the folder of a session whose conversation cannot be read, with a note on what it was doing. */
function StartIn({ c, keyOf, row }: { c: Controller; keyOf: string; row: SessionRow }) {
  const [options, setOptions] = useState<HandoffOptions | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [dest, setDest] = useState('');
  const [note, setNote] = useState('');
  const [starting, setStarting] = useState(false);
  const label = row.title ?? row.id.slice(0, 8);
  useEffect(() => {
    let live = true;
    api.handoffOptions(keyOf, row.id).then((got) => {
      if (!live) return;
      setOptions(got);
      setDest(got.destinations[0]?.key ?? '');
      setNote(got.note);
    }, (error) => { if (live) setProblem(errorText(error)); });
    return () => { live = false; };
  }, [keyOf, row.id]);
  const start = async (): Promise<void> => {
    if (starting || !options) return;
    setStarting(true);
    setProblem(null);
    const chosen = options.destinations.find((item) => item.key === dest)?.label ?? dest;
    try {
      const result = await api.handoff(keyOf, row.id, dest, note);
      c.close();
      c.report(`Started ${chosen} in ${result.cwd}; note saved to ${result.note_path}`);
    } catch (error) {
      setStarting(false);
      setProblem(errorText(error));
    }
  };
  const cwd = options?.cwd ?? row.cwd;
  let body;
  if (!options) body = problem ? <Note tone="error" title={problem} /> : <Progress aria-label="Composing the note" value={null} />;
  else if (options.refusal || !options.destinations.length) body = <Note tone="warning" title={options.refusal ?? 'No installation mukade can start from a note.'} />;
  else {
    body = (
      <form id="start-in" onSubmit={(event: FormEvent) => { event.preventDefault(); void start(); }}>
        <Column gap="md">
          <FormField label="Harness"><Select options={options.destinations.map((item) => ({ value: item.key, label: item.label }))} value={dest} onValueChange={setDest} /></FormField>
          <FormField label="Note · the first thing it reads"><Textarea mono={false} rows={8} value={note} onChange={(e) => setNote(e.target.value)} onKeyDown={(event) => { if (event.key === 'Enter' && (event.ctrlKey || event.metaKey)) { event.preventDefault(); void start(); } }} /></FormField>
          {cwd ? <Note tone="info" title={`It starts in ${cwd}. The note is saved in mukade’s data folder and the harness is told to read it.`} /> : null}
          {problem ? <Note tone="error" title={problem} /> : null}
        </Column>
      </form>
    );
  }
  const ready = options !== null && !options.refusal && options.destinations.length > 0;
  return (
    <Dialog
      open
      onOpenChange={(open) => { if (!open && !starting) c.close(); }}
      title="Start in…"
      description={label}
      width="lg"
      footer={(
        <>
          <Spacer />
          <Button variant="outline" onClick={() => { if (!starting) c.close(); }}>{ready ? 'Cancel' : 'Close'}</Button>
          {ready ? <Button type="submit" form="start-in" variant="default" loading={starting} loadingText="Starting…">Start</Button> : null}
        </>
      )}
    >
      {body}
    </Dialog>
  );
}

function Keys({ c }: { c: Controller }) {
  const commands = useCommands().filter((command) => command.keys);
  const lists: [string, string][] = [
    ['↑ ↓ · J K', 'move in the list of sessions; in the reader, scroll'], ['Home · End · Page Up · Page Down', 'first · last · by ten in the list; page in the reader'],
    ['Enter', 'open the transcript of the selected session'], ['Space', 'fold a parent session'], ['Esc', 'close the top layer, leave a field, close the reader'],
    ['F6 · Shift+F6', 'next · previous region'], ['Enter in the composer', 'send; Shift+Enter is a new line'],
  ];
  return (
    <Dialog open onOpenChange={(open) => { if (!open) c.close(); }} title="Keys" description="The mouse works everywhere too" width="lg" footer={<><Spacer /><Button variant="default" onClick={() => c.close()}>Close</Button></>}>
      <DescriptionList
        aria-label="Keys"
        items={[
          ...commands.map((command) => ({ name: command.shortcut ?? command.keys, value: command.label })),
          ...lists.map(([keys, what]) => ({ name: keys, value: what })),
        ]}
      />
    </Dialog>
  );
}

/** Settings are a dialog, opened from the palette: not a view of their own. The theme is the palette's; glass is the window's, where there is one. */
function Settings({ c }: { c: Controller }) {
  const [glass, setGlass] = useGlass();
  const own = useWindow().own;
  const palette = useCommandPalette();
  return (
    <Dialog open onOpenChange={(open) => { if (!open) c.close(); }} title="Settings" description="Appearance and About" width="lg" footer={<><Spacer /><Button variant="default" onClick={() => c.close()}>Close</Button></>}>
      <Column gap="lg">
        <Section title="Appearance" level={3} icon={<Icon name="appearance" />}>
          <SettingRow
            label="Theme"
            description="Light or dark with the switch of the title bar; the system’s look, with light and dark, is in the palette of every app."
            control={<Button variant="outline" size="sm" onClick={() => { c.close(); palette.setOpen(true); }}>Open the commands</Button>}
          />
          {/* A browser has no glass: the setting is in a window of its own. */}
          {own ? (
            <SettingRow
              label="Glass window"
              description="Windows’ acrylic behind the page: a blurred, tinted copy of the desktop. It changes at once; the same choice is the command Glass of the palette."
              control={<Switch aria-label="Glass window" checked={glass} onCheckedChange={setGlass} />}
            />
          ) : null}
        </Section>
        <Section title="About Mukade" level={3} icon={<Icon name="info" />}>
          <DescriptionList aria-label="About Mukade" items={[{ name: 'Version', value: c.programVersion || '—' }]} />
        </Section>
      </Column>
    </Dialog>
  );
}

function NewRoom({ c }: { c: Controller }) {
  const chat = c.chat;
  const [name, setName] = useState('');
  const [running, setRunning] = useState(false);
  const value = name.trim();
  const refusal = value ? roomNameRefusal(value) : null;
  const existing = value ? roomNamed(chat.rooms, value) : undefined;
  const run = async (): Promise<void> => {
    if (running || !value || refusal || existing) return;
    setRunning(true);
    const id = await chat.openNewRoom(value);
    setRunning(false);
    if (id) c.close();
  };
  return (
    <Dialog
      open
      onOpenChange={(open) => { if (!open && !running) c.close(); }}
      title="New room"
      footer={<><Spacer />{CANCEL(c)}<Button type="submit" form="new-room" variant="default" disabled={!value || refusal !== null || existing !== undefined} loading={running} loadingText="Opening…">Open the room</Button></>}
    >
      <form id="new-room" onSubmit={(event: FormEvent) => { event.preventDefault(); void run(); }}>
        <Column gap="sm">
          <FormField label="Name" help="Lowercase letters, digits, - and _." error={refusal ?? undefined}>
            <Input value={name} placeholder="design-review" autoComplete="off" spellCheck={false} onChange={(e) => setName(e.target.value)} />
          </FormField>
          {existing && existing.closed_at === null ? (
            <Cluster gap="sm"><Text>{value} is open already.</Text><Button variant="outline" size="sm" onClick={() => { c.close(); chat.pickRoom(existing.room_id); }}>Go to it</Button></Cluster>
          ) : null}
          {existing && existing.closed_at !== null ? (
            <Cluster gap="sm"><Text>{value} is closed.</Text><Button variant="outline" size="sm" onClick={() => { c.close(); chat.pickRoom(existing.room_id); void chat.reopen(existing); }}>Reopen it</Button></Cluster>
          ) : null}
          {chat.writeError ? <Note tone="error" title={chat.writeError} /> : null}
        </Column>
      </form>
    </Dialog>
  );
}

/** Run an agent in the shown room, in a new terminal, with the command shown first. */
function Launch({ c }: { c: Controller }) {
  const chat = c.chat;
  const room = chat.current();
  const [harness, setHarness] = useState<string>('claude_code');
  const [role, setRole] = useState<string>('peer');
  const [listen, setListen] = useState('300');
  const [folder, setFolder] = useState('');
  const [problem, setProblem] = useState<string | null>(null);
  const [running, setRunning] = useState(false);
  if (!room) return null;
  const seconds = Number(listen);
  const valid = Number.isInteger(seconds) && seconds >= 0 && seconds <= 3600;
  const preview = commandLine(['mukade', 'chat', 'launch', harness, '--room', room.name, '--role', role, '--listen', String(valid ? seconds : 300)]);
  const run = async (): Promise<void> => {
    if (!valid || running) return;
    setRunning(true);
    try {
      await chat.launch(harness, role, seconds, folder.trim() || undefined);
      c.close();
    } catch (error) {
      setRunning(false);
      setProblem(errorText(error));
    }
  };
  return (
    <Dialog
      open
      onOpenChange={(open) => { if (!open && !running) c.close(); }}
      title="Launch an agent"
      description={`into ${room.name}`}
      width="lg"
      footer={<><Spacer />{CANCEL(c)}<Button type="submit" form="launch" variant="default" disabled={!valid} loading={running} loadingText="Launching…"><Icon name="code" />Launch</Button></>}
    >
      <form id="launch" onSubmit={(event: FormEvent) => { event.preventDefault(); void run(); }}>
        <Column gap="md">
          <Column gap="xs">
            <Text variant="strong">Harness</Text>
            <ToggleGroup aria-label="Harness" value={harness} onValueChange={setHarness}>
              {HARNESSES.map(([key, label]) => <ToggleGroupItem key={key} value={key}>{label}</ToggleGroupItem>)}
            </ToggleGroup>
          </Column>
          <Column gap="xs">
            <Text variant="strong">Role</Text>
            <ToggleGroup aria-label="Role" value={role} onValueChange={setRole}>
              {ROLES.map(([key, label]) => <ToggleGroupItem key={key} value={key}>{label}</ToggleGroupItem>)}
            </ToggleGroup>
          </Column>
          <Cluster gap="md" align="start">
            <FormField label="Listens · seconds between turns" error={valid ? undefined : 'From 0 to 3600 seconds.'}>
              <Input short="xs" type="number" min={0} max={3600} step={30} value={listen} onChange={(e) => setListen(e.target.value)} />
            </FormField>
            <Cell grow>
              <FormField label="Folder">
                <Input value={folder} placeholder="the folder the terminal opens in" autoComplete="off" spellCheck={false} onChange={(e) => setFolder(e.target.value)} />
              </FormField>
            </Cell>
            <Cell align="end"><Button variant="outline" onClick={() => { void api.pick('folder').then((picked) => { if (picked) setFolder(picked); }, () => {}); }}>Choose…</Button></Cell>
          </Cluster>
          <Column gap="xs"><Text variant="strong">It runs, in a new terminal</Text><Text as="div" mono pre>{preview}</Text></Column>
          <Note tone="info" title={harness === 'codex' ? 'Codex runs the launch hooks only once they are trusted: mukade chat trust codex, once.' : 'The window does not join for the agent: its member appears when it joins.'} />
          {problem ? <Note tone="error" title={problem} /> : null}
        </Column>
      </form>
    </Dialog>
  );
}

/** Install a harness: its own command in a terminal, or a sandbox of its own that npm makes. */
function Install({ c, harness }: { c: Controller; harness: HarnessView }) {
  const h = c.harnesses;
  const [name, setName] = useState('try');
  const [problem, setProblem] = useState<string | null>(null);
  const installing = h.installing.has(harness.id);
  const valid = /^[a-z0-9_-]{1,32}$/.test(name.trim());
  const run = async (): Promise<void> => {
    if (installing) return;
    if (!valid) { setProblem('Lowercase letters, digits, - and _ (up to 32).'); return; }
    setProblem(null);
    try {
      await h.installSandbox(harness, name.trim());
      c.close();
    } catch (error) {
      setProblem(errorText(error));
    }
  };
  return (
    <Dialog open onOpenChange={(open) => { if (!open) c.close(); }} title={`Install ${harness.display}`} width="lg" footer={<><Spacer /><Button variant="outline" onClick={() => c.close()}>Close</Button></>}>
      <Column gap="lg">
        <Section title="On the system" level={3} icon={<Icon name="device" />}>
          <Column gap="sm">
            {harness.install_command ? <Text as="div" mono pre>{harness.install_command}</Text> : <Note tone="info" title={`${harness.display} installs with its own script; its page in docs/harnesses says how.`} />}
            <Note tone="info" title={harness.status === 'missing' ? 'It runs in a new terminal, with the harness’s own installer; check again when it ends.' : `${harness.display} is already installed: the same command updates it. It runs in a new terminal; check again when it ends.`} />
            {harness.install_command ? <Cluster><Button variant="outline" onClick={() => { void h.installSystem(harness).then((ok) => { if (ok) c.close(); }); }}><Icon name="code" />Run in a terminal</Button></Cluster> : null}
          </Column>
        </Section>
        <Section title="In a sandbox" level={3} icon={<Icon name="package" />}>
          {harness.sandbox_package ? (
            <form onSubmit={(event: FormEvent) => { event.preventDefault(); void run(); }}>
              <Column gap="sm">
                <Text as="p">npm installs <Text mono>{harness.sandbox_package}</Text> into <Text mono>{`mukade-data/sandboxes/${harness.id}-<name>`}</Text>, with a store of its own: it touches neither the installed harness nor its sessions. Hops can land in it, and Run opens it in a terminal. It needs Node.js 22 or later, and its own sign-in.</Text>
                <FormField label="Name" error={problem ?? undefined}><Input short="md" value={name} autoComplete="off" spellCheck={false} onChange={(e) => setName(e.target.value)} /></FormField>
                {installing ? <Text variant="muted">npm is installing {harness.sandbox_package}; this can take a few minutes. You can close this dialog.</Text> : null}
                <Cluster><Button type="submit" variant="default" loading={installing} loadingText="Installing…"><Icon name="download" />Install in a sandbox</Button></Cluster>
              </Column>
            </form>
          ) : <Note tone="info" title={`${harness.display} is not an npm package: it has no sandbox.`} />}
        </Section>
      </Column>
    </Dialog>
  );
}

/** The dialog that is open, if any. */
export function Dialogs({ c }: { c: Controller }) {
  const dialog: DialogState | null = c.dialog;
  if (dialog === null) return null;
  switch (dialog.kind) {
    case 'hop': return <WriteDialog key="hop" c={c} kind="hop" hop={dialog} />;
    case 'import': return <WriteDialog key="import" c={c} kind="import" importing={dialog} />;
    case 'recover': return <WriteDialog key="recover" c={c} kind="recover" />;
    case 'exported': return <Exported c={c} exported={dialog.exported} />;
    case 'resume': return <ResumeRefused c={c} result={dialog.result} />;
    case 'start-in': return <StartIn c={c} keyOf={dialog.key} row={dialog.row} />;
    case 'keys': return <Keys c={c} />;
    case 'search': return <SearchDialog c={c} />;
    case 'settings': return <Settings c={c} />;
    case 'new-room': return <NewRoom c={c} />;
    case 'launch': return <Launch c={c} />;
    case 'install': return <Install c={c} harness={dialog.harness} />;
    case 'confirm':
      return (
        <ConfirmDialog
          open
          onOpenChange={(open) => { if (!open) c.close(); }}
          title={dialog.title}
          description={dialog.description}
          confirmLabel={dialog.confirmLabel}
          destructive={dialog.destructive ?? true}
          onConfirm={() => { c.close(); void dialog.run(); }}
        />
      );
  }
}
