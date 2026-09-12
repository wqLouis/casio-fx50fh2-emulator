/**
 * Which theme is in force.
 *
 * A theme is a class on `<html>` — `theme-catppuccin-mocha` and friends — and
 * that is the only switch. `layout.css` holds the palettes; the class is put
 * there before the first paint by an inline script in `app.html` so the page
 * never flashes the wrong one, and this store takes over from that script once
 * the app is running and remembers what was chosen.
 *
 * The list, the validation and the fallbacks are in `theme.ts` so they can be
 * tested without the Svelte compiler. This file only applies them.
 *
 * `restore()` must be called from `onMount`: every route is prerendered, and
 * `matchMedia` does not exist on the server.
 */

import {
	DEFAULT_DARK,
	DEFAULT_LIGHT,
	THEMES,
	THEME_STORAGE_KEY,
	counterpartOf,
	isThemeId,
	themeById,
	type ThemeChoice,
	type ThemeId,
	type ThemeInfo
} from './theme';

// Re-exported so a component needs one import, not two.
export { THEMES, THEME_STORAGE_KEY, isThemeId, type ThemeChoice, type ThemeId, type ThemeInfo };

class ThemeStore {
	/** What the user picked. `system` follows the operating system. */
	choice = $state<ThemeChoice>('system');
	/** Whether the operating system currently asks for dark. */
	systemDark = $state(false);

	/** The theme actually in force. */
	resolved = $derived<ThemeId>(
		this.choice === 'system' ? (this.systemDark ? DEFAULT_DARK : DEFAULT_LIGHT) : this.choice
	);

	/** Its entry in the list, for the label and the editor's dark flag. */
	info = $derived<ThemeInfo>(themeById(this.resolved));

	/** `light` or `dark`, which is all the editor needs to know. */
	appearance = $derived<'light' | 'dark'>(this.info.dark ? 'dark' : 'light');

	#media: MediaQueryList | null = null;

	/**
	 * Adopt the stored choice and start following the system setting.
	 *
	 * Safe to call more than once, so a component may call it without
	 * coordinating with any other.
	 */
	restore(): void {
		if (typeof window === 'undefined') return;

		if (this.#media === null) {
			this.#media = window.matchMedia('(prefers-color-scheme: dark)');
			// Only matters while the choice is `system`; `#apply` decides.
			this.#media.addEventListener('change', (event) => {
				this.systemDark = event.matches;
				this.#apply();
			});
		}
		this.systemDark = this.#media.matches;

		try {
			const stored = localStorage.getItem(THEME_STORAGE_KEY);
			if (stored === 'system' || isThemeId(stored)) this.choice = stored;
		} catch {
			// Storage can throw when it is disabled; the default still works.
		}
		this.#apply();
	}

	/** Choose a theme, or hand control back to the operating system. */
	set(choice: ThemeChoice): void {
		this.choice = choice;
		try {
			localStorage.setItem(THEME_STORAGE_KEY, choice);
		} catch {
			// Session-only, which is a reasonable degradation.
		}
		this.#apply();
	}

	/** Flip between light and dark, keeping the theme family where there is one. */
	toggle(): void {
		this.set(counterpartOf(this.resolved));
	}

	#apply(): void {
		if (typeof document === 'undefined') return;
		const root = document.documentElement;
		for (const entry of THEMES) {
			root.classList.toggle(entry.id, entry.id === this.resolved);
		}
		// `color-scheme` is declared per theme in `layout.css`, so it is not set
		// here: two sources for it would be one too many.
	}
}

export const theme = new ThemeStore();
