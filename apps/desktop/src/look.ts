// The look of the window: Smooth (the default) or Oni (the look Mukade was born with). The choice is
// an attribute on the root (`data-look`): smooth.css is scoped to it, the other stylesheets are Oni.
// Each look keeps its own theme: Oni is dark unless light was chosen, Smooth follows the system
// unless light or dark was chosen.

export type Look = 'smooth' | 'oni';
export type Theme = 'system' | 'light' | 'dark';
export const LOOK_KEY = 'mukade.look';
export const THEME_KEY: Record<Look, string> = { oni: 'mukade.theme', smooth: 'mukade.smooth.theme' };
/** The themes each look has, its default first. */
export const THEMES: Record<Look, readonly Theme[]> = { oni: ['dark', 'light'], smooth: ['system', 'light', 'dark'] };

/** The look a stored value stands for: Oni only when it was chosen. */
export const storedLook = (raw: string | null): Look => (raw === 'oni' ? 'oni' : 'smooth');
/** The theme a stored value stands for in a look: anything the look does not have is its first. */
export const storedTheme = (look: Look, raw: string | null): Theme => THEMES[look].find((theme) => theme === raw) ?? THEMES[look][0];
/** The theme the Theme button goes to next. */
export const nextTheme = (look: Look, theme: Theme): Theme => THEMES[look][(THEMES[look].indexOf(theme) + 1) % THEMES[look].length];
/** What a theme paints: `system` is the system's own. */
export const paints = (theme: Theme, systemLight: boolean): 'light' | 'dark' => (theme === 'system' ? (systemLight ? 'light' : 'dark') : theme);
