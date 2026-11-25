// The mark of a harness: its own logo, a picture the app ships (src/logos), in the one place a person looks for it. The monochrome
// logos (OpenCode, Pi) are drawn in one neutral colour that reads on the sheet in both schemes; a harness with no logo of its own
// (Hermes, DeepSeek Harness, a member that is the window itself) has one of Sigrid's icons.
import { Icon, Logo } from '@acaprino/sigrid/primitives';
import { harnessKey } from '../chatmodel.ts';
import claudeCode from '../logos/claude_code.svg';
import codex from '../logos/codex.svg';
import opencode from '../logos/opencode.svg';
import pi from '../logos/pi.svg';

const LOGOS: Record<string, { src: string; name: string }> = {
  claude_code: { src: claudeCode, name: 'Claude Code' },
  codex: { src: codex, name: 'Codex' },
  opencode: { src: opencode, name: 'OpenCode' },
  pi: { src: pi, name: 'Pi' },
};

/** The address of a harness's logo, for the parts that take a picture (a graph's node), or undefined. */
export const logoOf = (harness: string): string | undefined => LOGOS[harnessKey(harness)]?.src;

/** The harness's mark, beside its name (so the picture is decorative). */
export function HarnessMark({ harness, size }: { harness: string; size?: 'sm' | 'md' }) {
  const key = harnessKey(harness);
  const logo = LOGOS[key];
  if (logo) return <Logo src={logo.src} name={logo.name} decorative size={size} icon="package" />;
  return <Icon name={key === 'desktop' ? 'device' : 'package'} />;
}
