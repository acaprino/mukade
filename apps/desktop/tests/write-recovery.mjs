// The IPC substitute lives outside the page so an actual page reload drops all renderer state.
import assert from 'node:assert/strict';

export function holdImports(answer) {
  let current = null;
  let requests = 0;
  let sessions = 0;
  return {
    async invoke(command, args) {
      if (command === 'write_status') return current;
      if (command === 'acknowledge_write') {
        if (current?.id === args.id && current.status !== 'running') current = null;
        return;
      }
      if (command !== 'import') return answer(command, args);
      requests++;
      current ??= { id: requests, kind: 'import', status: 'running' };
      return current;
    },
    fail() {
      assert.equal(current.status, 'running');
      current = { ...current, status: 'failed', error: { kind: 'failed', message: 'temporary failure' } };
    },
    finish() {
      assert.equal(current.status, 'running');
      current = { ...current, status: 'completed', receipt: {
        dest_id: 'import-' + ++sessions, dest_key: 'pi', dest_label: 'Pi', location: 'session.jsonl',
        interactive: [], headless: [], losses: [], children: [],
      } };
    },
    get requests() { return requests; },
    get sessions() { return sessions; },
    get current() { return current; },
  };
}

export async function checkWriteRecovery(page, writes) {
  const done = page.getByRole('dialog', { name: 'Done', exact: true });
  const recovered = page.getByRole('dialog', { name: 'Import', exact: true });
  assert.equal(writes.requests, 2); // One failed write and one explicit retry.
  await page.reload();
  await recovered.getByText('In progress…', { exact: true }).first().waitFor();
  // The key of Import opens no second dialog over the one that waits, and the form is not there: the write is Rust's until it is told.
  await page.keyboard.press('Escape');
  await page.keyboard.press('i');
  await page.keyboard.press('Enter');
  await recovered.getByRole('progressbar').waitFor();
  assert.equal(await recovered.getByRole('textbox', { name: 'Document' }).count(), 0);
  assert.equal(writes.requests, 2);

  writes.finish();
  await done.getByText('import-1', { exact: true }).waitFor();
  await page.reload(); // Losing the receipt before dismissal restores the same one.
  await done.getByText('import-1', { exact: true }).waitFor();
  assert.equal(await done.count(), 1);
  assert.equal(writes.sessions, 1);
  assert.equal(writes.requests, 2);
  const acknowledged = page.waitForFunction(() => window.testIpc.acknowledged);
  acknowledged.catch(() => {}); // a failure before it is awaited must not hide itself as an unhandled rejection
  await done.getByRole('button', { name: 'Close', exact: true }).first().click();
  await acknowledged;
  assert.equal(writes.current, null);
  await page.reload();
  await page.getByRole('tree', { name: 'Sessions' }).getByRole('treeitem').first().waitFor();
  await page.waitForFunction(() => window.testIpc.checked);
  assert.equal(await page.getByRole('dialog').count(), 0);

  await page.getByRole('button', { name: 'Import', exact: true }).click();
  const form = page.getByRole('dialog', { name: 'Import', exact: true });
  const path = form.getByRole('textbox', { name: 'Document' });
  await path.fill('invalid.json');
  await path.press('Enter');
  await page.waitForFunction(() => window.testIpc.imports === 1);
  writes.fail();
  await page.reload();
  await recovered.getByText('temporary failure', { exact: false }).first().waitFor();
  assert.equal(writes.requests, 3);
  assert.equal(writes.sessions, 1);
  await recovered.getByRole('button', { name: 'Close', exact: true }).first().click();
  await page.waitForFunction(() => window.testIpc.acknowledged);
}
