/**
 * The reactive layout store, and its one concession to the browser.
 *
 * The pure shape lives in `layout.ts`; this only holds it in runes and talks to
 * storage. Storage is read in `restore()` rather than at construction because
 * this module is imported during prerendering, where `localStorage` does not
 * exist. The shell calls `restore()` from `onMount`, which is the first moment
 * there is a browser to read.
 *
 * Every storage call is guarded three times over: the global may be absent
 * (server, tests), merely touching it may throw (some privacy modes), and
 * reading or writing may throw (quota, blocked). None of those may stop the
 * workbench, so all three fall back to an in-memory layout.
 */

import {
	LAYOUT_STORAGE_KEY,
	LAYOUT_VERSION,
	emptyLayout,
	parseLayout,
	serializeLayout,
	withPanel,
	withSize,
	type LayoutData
} from './layout';

/** How the store is configured. Tests inject a fake `Storage`. */
export interface LayoutStoreOptions {
	/** The `localStorage` key. Defaults to [LAYOUT_STORAGE_KEY]. */
	key?: string;
	/**
	 * The storage to use. `undefined` auto-detects `localStorage`; `null`
	 * disables persistence entirely.
	 */
	storage?: Storage | null;
}

/** `localStorage`, or `null` when there is none and asking is unsafe. */
function safeLocalStorage(): Storage | null {
	try {
		return typeof localStorage === 'undefined' ? null : localStorage;
	} catch {
		return null;
	}
}

/** Which panels are open and how the splits are sized. */
export class LayoutStore {
	/** Panel id to open/closed. */
	panels = $state<Record<string, boolean>>({});

	/** Split id to first-pane size in pixels. */
	sizes = $state<Record<string, number>>({});

	readonly #key: string;
	readonly #storage: Storage | null | undefined;
	#timer: ReturnType<typeof setTimeout> | null = null;

	constructor(options: LayoutStoreOptions = {}) {
		this.#key = options.key ?? LAYOUT_STORAGE_KEY;
		this.#storage = options.storage;
	}

	/**
	 * Load the persisted layout.
	 *
	 * Safe anywhere, and a no-op when there is no storage. Call it from the
	 * shell's `onMount`, never at module scope.
	 */
	restore(): void {
		const storage = this.#resolveStorage();
		if (!storage) return;
		const data = parseLayout(this.#read(storage));
		this.panels = data.panels;
		this.sizes = data.sizes;
	}

	/** Whether a panel is open, falling back to `fallback` when unset. */
	isPanelVisible(id: string, fallback = true): boolean {
		const value = this.panels[id];
		return typeof value === 'boolean' ? value : fallback;
	}

	/** Set a panel's visibility and persist. */
	setPanel(id: string, visible: boolean): void {
		if (id.length === 0) return;
		this.panels = withPanel(this.snapshot(), id, visible).panels;
		this.#schedule();
	}

	/** Flip a panel and return its new visibility. */
	togglePanel(id: string): boolean {
		const next = !this.isPanelVisible(id);
		this.setPanel(id, next);
		return next;
	}

	/** A split's remembered size, or `fallback` when it has none. */
	sizeFor(id: string, fallback: number | null = null): number | null {
		const value = this.sizes[id];
		return typeof value === 'number' && Number.isFinite(value) ? value : fallback;
	}

	/** Remember a split's size and persist. */
	setSize(id: string, size: number): void {
		if (id.length === 0 || !Number.isFinite(size) || size < 0) return;
		this.sizes = withSize(this.snapshot(), id, size).sizes;
		this.#schedule();
	}

	/** The current layout as a plain object. */
	snapshot(): LayoutData {
		return { version: LAYOUT_VERSION, panels: { ...this.panels }, sizes: { ...this.sizes } };
	}

	/** Write now, cancelling any debounced write. */
	persist(): void {
		this.#cancel();
		const storage = this.#resolveStorage();
		if (!storage) return;
		try {
			storage.setItem(this.#key, serializeLayout(this.snapshot()));
		} catch {
			// Private mode or quota. The session still works; only reload is lost.
		}
	}

	/** Forget everything and persist the empty layout. */
	reset(): void {
		const empty = emptyLayout();
		this.panels = empty.panels;
		this.sizes = empty.sizes;
		this.persist();
	}

	#resolveStorage(): Storage | null {
		if (this.#storage !== undefined) return this.#storage;
		return safeLocalStorage();
	}

	#read(storage: Storage): string | null {
		try {
			return storage.getItem(this.#key);
		} catch {
			return null;
		}
	}

	#schedule(): void {
		// A drag fires `setSize` on every pointer move; coalesce those into one
		// write so storage is not hammered. `setTimeout` is always present in a
		// browser, but be defensive anyway.
		if (typeof setTimeout !== 'function') {
			this.persist();
			return;
		}
		this.#cancel();
		this.#timer = setTimeout(() => {
			this.#timer = null;
			this.persist();
		}, 150);
	}

	#cancel(): void {
		if (this.#timer !== null) {
			clearTimeout(this.#timer);
			this.#timer = null;
		}
	}
}

/**
 * The workbench's layout.
 *
 * Constructing it touches nothing; call `layout.restore()` in `onMount`.
 */
export const layout = new LayoutStore();
