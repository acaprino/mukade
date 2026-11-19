// The keyboard (spec §4.1): the TUI's keys, mapped to actions. Pure, so tests/keys.test.ts checks
// it without a window.

export type Action =
  | 'down' | 'up' | 'left' | 'right' | 'enter' | 'back'
  | 'hop' | 'export' | 'import' | 'resume' | 'turns'
  | 'top' | 'bottom' | 'pageUp' | 'pageDown'
  | 'scan' | 'reload' | 'search' | 'chat' | 'fold' | 'help';

export interface KeyLike { key: string; ctrlKey?: boolean; metaKey?: boolean; altKey?: boolean; shiftKey?: boolean }

/** The action of a key press; `inField` is true while a text field has the focus, where only
 *  Escape and Enter act. */
export function actionFor(event: KeyLike, inField: boolean): Action | null {
  const { key } = event;
  if (event.altKey) return null;
  if ((event.ctrlKey || event.metaKey) && key.toLowerCase() === 'f') return 'search';
  if (event.ctrlKey || event.metaKey) return null;
  if (key === 'Escape') return 'back';
  if (inField) return key === 'Enter' ? 'enter' : null;
  switch (key) {
    case 'ArrowDown': case 'j': return 'down';
    case 'ArrowUp': case 'k': return 'up';
    case 'ArrowLeft': return 'left';
    case 'ArrowRight': return 'right';
    case 'Enter': return 'enter';
    case 'h': return 'hop';
    case 'e': return 'export';
    case 'i': return 'import';
    case 'R': return 'resume';
    case 't': return 'turns';
    case 'g': return 'top';
    case 'G': return 'bottom';
    case 'PageUp': return 'pageUp';
    case 'PageDown': return 'pageDown';
    case 'o': return 'scan';
    case 'r': return 'reload';
    case '/': return 'search';
    case 'c': return 'chat';
    case ' ': return 'fold';
    case '?': return 'help';
    default: return null;
  }
}

/** The key hints the status line shows, per place. */
export const HINTS: Record<'sessions' | 'reader' | 'chat', [string, string][]> = {
  sessions: [['↑↓', 'move'], ['←→', 'column'], ['space', 'fold'], ['⏎', 'open'], ['h', 'hop'], ['e', 'export'], ['R', 'resume'], ['/', 'filter']],
  reader: [['j k', 'scroll'], ['g G', 'top · bottom'], ['PgUp PgDn', 'page'], ['t', 'turns'], ['h', 'hop'], ['esc', 'back']],
  chat: [['↑↓', 'move'], ['←→', 'column'], ['/', 'filter'], ['c', 'sessions']],
};
