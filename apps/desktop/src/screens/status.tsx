// The status row: what the window says (a badge and a sentence) and the job that runs. One live region: the shell reads it when the
// phrase changes. The counts of the stores are at its end.
import { Badge, Spacer, Text } from '@acaprino/sigrid/primitives';
import type { State } from '../state.ts';

export function Status({ state }: { state: State }) {
  const working = state.job !== null;
  const failed = !working && state.statusError;
  return (
    <>
      <Badge tone={working ? 'info' : failed ? 'danger' : 'success'}>{working ? 'Working' : failed ? 'Error' : 'Ready'}</Badge>
      <Text truncate>{state.job ?? state.status}</Text>
      <Spacer />
      <Text variant="muted">{state.counts}</Text>
    </>
  );
}
