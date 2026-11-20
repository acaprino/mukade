import type { Look, ThemeChoice as Theme } from '@acaprino/imago/shell';
export type { Look, Theme };
export const LOOK_KEY = 'mukade.look';
export const THEME_KEY: Record<Look, string> = { oni: 'mukade.theme', smooth: 'mukade.smooth.theme' };
/** The themes each look has, its default first. */
export const THEMES: Record<Look, readonly Theme[]> = { oni: ['dark', 'light'], smooth: ['system', 'light', 'dark'] };

/** The theme a stored value stands for in a look: anything the look does not have is its first. */
export const storedTheme = (look: Look, raw: string | null): Theme => THEMES[look].find((theme) => theme === raw) ?? THEMES[look][0];
/** The theme the Theme button goes to next. */
export const nextTheme = (look: Look, theme: Theme): Theme => THEMES[look][(THEMES[look].indexOf(theme) + 1) % THEMES[look].length];
