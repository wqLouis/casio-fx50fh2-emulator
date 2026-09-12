/**
 * The theme list, and what makes a stored value a valid one.
 *
 * This half is deliberately free of runes and of the DOM, so `bun test` can
 * check it — including against `layout.css`, which is where the palettes
 * actually live. The store that applies a theme is `theme.svelte.ts`; it imports
 * from here, so there is exactly one list.
 *
 * The colours themselves are in CSS and are **not** repeated here. That is the
 * point of the split: a theme is a class name in both files, and the test asserts
 * the two stay in step in both directions.
 */

/** The shape the picker needs. The colours are in `layout.css`, not here. */
export interface ThemeInfo {
	/** The class applied to `<html>`, and the value stored in `localStorage`. */
	id: string;
	label: string;
	/** Whether the palette is dark, for the editor's own dark-mode defaults. */
	dark: boolean;
}

/**
 * Every theme, in picker order: the two house themes, then the ports.
 *
 * `id` must match a class in `layout.css`.
 */
export const THEMES = [
	{ id: 'theme-fx50-dark', label: 'fx50 Dark', dark: true },
	{ id: 'theme-fx50-light', label: 'fx50 Light', dark: false },
	{ id: 'theme-catppuccin-latte', label: 'Catppuccin Latte', dark: false },
	{ id: 'theme-catppuccin-frappe', label: 'Catppuccin Frappé', dark: true },
	{ id: 'theme-catppuccin-macchiato', label: 'Catppuccin Macchiato', dark: true },
	{ id: 'theme-catppuccin-mocha', label: 'Catppuccin Mocha', dark: true },
	{ id: 'theme-gruvbox-dark', label: 'Gruvbox Dark', dark: true },
	{ id: 'theme-gruvbox-light', label: 'Gruvbox Light', dark: false },
	{ id: 'theme-nord', label: 'Nord', dark: true },
	{ id: 'theme-solarized-dark', label: 'Solarized Dark', dark: true },
	{ id: 'theme-solarized-light', label: 'Solarized Light', dark: false },
	{ id: 'theme-tokyo-night', label: 'Tokyo Night', dark: true },
	{ id: 'theme-dracula', label: 'Dracula', dark: true },
	{ id: 'theme-rose-pine', label: 'Rosé Pine', dark: true }
] as const satisfies readonly ThemeInfo[];

export type ThemeId = (typeof THEMES)[number]['id'];

/** Either one theme, or whatever the operating system asks for. */
export type ThemeChoice = 'system' | ThemeId;

/** The theme `system` resolves to on each side. */
export const DEFAULT_DARK: ThemeId = 'theme-fx50-dark';
export const DEFAULT_LIGHT: ThemeId = 'theme-fx50-light';

/** Shared with the inline script in `app.html`; changing it means changing both. */
export const THEME_STORAGE_KEY = 'fx50.theme';

/**
 * Every palette variable a theme must define.
 *
 * Ten shades from the page colour up to the brightest text, then the accents.
 * Missing one of these is invisible in review — the utility that reads it falls
 * back to an unstyled default rather than erroring — so the test checks the list
 * against every theme.
 */
export const REQUIRED_PALETTE_VARS = [
	'--t-page',
	'--t-surface',
	'--t-line',
	'--t-hover',
	'--t-off',
	'--t-dim',
	'--t-muted',
	'--t-body',
	'--t-strong',
	'--t-bright',
	'--t-brightest',
	'--t-danger',
	'--t-warn',
	'--t-ok',
	'--t-info',
	'--t-mauve',
	'--t-teal',
	'--t-orange',
	'--t-pink'
] as const;

const IDS: ReadonlySet<string> = new Set(THEMES.map((entry) => entry.id));

/** Whether a stored or requested value names a theme that exists. */
export function isThemeId(value: string | null | undefined): value is ThemeId {
	return value !== null && value !== undefined && IDS.has(value);
}

/** The entry for an id, with the first theme as a total fallback. */
export function themeById(id: ThemeId): ThemeInfo {
	return THEMES.find((entry) => entry.id === id) ?? THEMES[0];
}

/**
 * Themes that share a light side, and what that side is.
 *
 * A table rather than a rule, because the names do not encode a direction:
 * Catppuccin's flavours are Latte, Frappé, Macchiato and Mocha, and only the
 * first is light. Guessing from the name would work for Gruvbox and fail for
 * exactly the family most likely to be in use. Catppuccin has three dark
 * flavours to one light, so a dark-to-light flip lands on Latte from any of
 * them, and a light-to-dark flip goes to Mocha.
 */
const FAMILIES: readonly { light: ThemeId; darks: readonly ThemeId[] }[] = [
	{ light: 'theme-fx50-light', darks: ['theme-fx50-dark'] },
	{
		light: 'theme-catppuccin-latte',
		darks: ['theme-catppuccin-mocha', 'theme-catppuccin-frappe', 'theme-catppuccin-macchiato']
	},
	{ light: 'theme-gruvbox-light', darks: ['theme-gruvbox-dark'] }
];

/**
 * The other side of a theme, for the light/dark shortcut.
 *
 * Flipping appearance has to do something sensible when the current theme is
 * Catppuccin Mocha rather than a house theme, and the useful answer is that
 * family's other side — Mocha becomes Latte, not "the default light theme,
 * discarding what you picked". Themes with no twin (Nord, Dracula, Tokyo Night,
 * Rosé Pine, and the two Solarized halves, which are listed independently) fall
 * back to the house theme of the other appearance, because inventing a variant
 * would be a lie.
 */
export function counterpartOf(id: ThemeId): ThemeId {
	for (const family of FAMILIES) {
		if (id === family.light) return family.darks[0];
		if (family.darks.includes(id)) return family.light;
	}
	return themeById(id).dark ? DEFAULT_LIGHT : DEFAULT_DARK;
}
