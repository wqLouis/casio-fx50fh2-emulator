/**
 * Light, dark, and the choice between them.
 *
 * The theme is a class on `<html>` — `light` or `dark`, never both and never
 * neither — because that is what the palette in `layout.css` keys off. The
 * class is put there before the first paint by an inline script in `app.html`,
 * so the page never flashes the wrong theme; this store's job is to take over
 * from that script once the app is running, and to remember what the user
 * picked.
 *
 * `restore()` must be called from `onMount`. Nothing here may touch a browser
 * global at module scope: every route is prerendered, and `matchMedia` does not
 * exist on the server.
 */

export type ThemeChoice = 'light' | 'dark' | 'system';
export type ResolvedTheme = 'light' | 'dark';

/** Shared with the inline script in `app.html`; changing it means changing both. */
export const THEME_STORAGE_KEY = 'fx50.theme';

function isChoice(value: string | null): value is ThemeChoice {
	return value === 'light' || value === 'dark' || value === 'system';
}

class ThemeStore {
	/** What the user picked. `system` follows the operating system. */
	choice = $state<ThemeChoice>('system');
	/** Whether the operating system currently asks for dark. */
	systemDark = $state(false);

	/** The theme actually in force. */
	resolved = $derived<ResolvedTheme>(
		this.choice === 'system' ? (this.systemDark ? 'dark' : 'light') : this.choice
	);

	#media: MediaQueryList | null = null;

	/**
	 * Adopt the stored choice and start following the system setting.
	 *
	 * Safe to call more than once — the media listener is registered once — so a
	 * component may call it without coordinating with any other.
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
			if (isChoice(stored)) this.choice = stored;
		} catch {
			// Storage can throw when it is disabled; the default still works.
		}
		this.#apply();
	}

	/** Choose explicitly. `system` hands control back to the operating system. */
	set(choice: ThemeChoice): void {
		this.choice = choice;
		try {
			localStorage.setItem(THEME_STORAGE_KEY, choice);
		} catch {
			// Session-only, which is a reasonable degradation.
		}
		this.#apply();
	}

	/** Flip between light and dark, leaving `system` behind for good. */
	toggle(): void {
		this.set(this.resolved === 'dark' ? 'light' : 'dark');
	}

	#apply(): void {
		if (typeof document === 'undefined') return;
		const dark = this.resolved === 'dark';
		const root = document.documentElement;
		root.classList.toggle('dark', dark);
		root.classList.toggle('light', !dark);
		// Form controls, scrollbars and the canvas behind the page.
		root.style.colorScheme = dark ? 'dark' : 'light';
	}
}

export const theme = new ThemeStore();
