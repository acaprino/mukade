import type { Look, ThemeChoice as Theme } from '@acaprino/imago/shell';
export type { Look, Theme };
export const LOOK_KEY = 'mukade.look';
export const THEME_KEY: Record<Look, string> = { oni: 'mukade.theme', smooth: 'mukade.smooth.theme' };
/** The theme each look starts with: ONI is dark, Smooth follows Windows. */
const DEFAULT_THEME: Record<Look, Theme> = { oni: 'dark', smooth: 'system' };

/** The theme a stored value stands for in a look: a value that is none of the three is the look's own. ONI with `system` is
 *  dark (`applyLook` resolves it), so the palette's "Follow Windows" is kept for it too. */
export const storedTheme = (look: Look, raw: string | null): Theme => (raw === 'system' || raw === 'light' || raw === 'dark' ? raw : DEFAULT_THEME[look]);
