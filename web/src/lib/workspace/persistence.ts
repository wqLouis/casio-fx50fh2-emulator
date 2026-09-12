/**
 * Remembering the directory handle across reloads.
 *
 * A `FileSystemDirectoryHandle` is structured-cloneable, so IndexedDB can hold
 * it while `localStorage` cannot. That is the whole reason `idb-keyval` is a
 * dependency: it is the standard tiny wrapper for exactly this one job.
 *
 * Every operation is best-effort. Browser storage can be unavailable (private
 * windows), full, or blocked, and none of that should stop the editor from
 * working for the session — it only costs the "reopen last folder" convenience.
 */

import { del, get, set } from 'idb-keyval';

const HANDLE_KEY = 'fx50.workspace.directoryHandle';

/**
 * The sliver of `idb-keyval` used here, behind an interface so a test can supply
 * a store without a browser. `idb-keyval` is the default.
 */
export interface HandleStore {
	get(key: string): Promise<unknown>;
	set(key: string, value: unknown): Promise<void>;
	del(key: string): Promise<void>;
}

const indexedDbStore: HandleStore = {
	get: (key) => get(key),
	set: (key, value) => set(key, value),
	del: (key) => del(key)
};

let injectedStore: HandleStore | null = null;

/** Replace the handle store, or pass `null` to restore the IndexedDB one. Tests use this. */
export function setHandleStore(next: HandleStore | null): void {
	injectedStore = next;
}

/**
 * The store to use, or `null` when there is nowhere to persist.
 *
 * IndexedDB is genuinely absent outside a browser (and in `bun test`), so it is
 * checked at call time rather than module scope: importing this file must never
 * touch `indexedDB`. An injected store is used regardless, which is how a test
 * exercises restore without a browser.
 */
function activeStore(): HandleStore | null {
	if (injectedStore) return injectedStore;
	return typeof indexedDB === 'undefined' ? null : indexedDbStore;
}

/** Persist the handle. Failure is swallowed; the session still works. */
export async function rememberHandle(handle: unknown): Promise<void> {
	const store = activeStore();
	if (!store) return;
	try {
		await store.set(HANDLE_KEY, handle);
	} catch {
		// Private mode or quota. Losing restore is acceptable; throwing is not.
	}
}

/** The remembered handle, or `null`. Never throws. */
export async function recallHandle(): Promise<unknown | null> {
	const store = activeStore();
	if (!store) return null;
	try {
		return (await store.get(HANDLE_KEY)) ?? null;
	} catch {
		return null;
	}
}

/** Forget the handle — used when it is stale beyond recovery. Never throws. */
export async function forgetHandle(): Promise<void> {
	const store = activeStore();
	if (!store) return;
	try {
		await store.del(HANDLE_KEY);
	} catch {
		// Nothing to do; the caller is already falling back.
	}
}

const WORKSPACE_KEY = 'fx50.workspace.files';

/**
 * A workspace kept in the browser, as stored.
 *
 * The files are held as plain text keyed by relative path — the same shape the
 * wasm API takes — so restoring one is a constructor call rather than a
 * re-import.
 */
export interface StoredWorkspace {
	name: string;
	files: Record<string, string>;
}

/** Persist a browser workspace. Failure is swallowed, as for the handle above. */
export async function rememberWorkspace(workspace: StoredWorkspace): Promise<void> {
	const store = activeStore();
	if (!store) return;
	try {
		await store.set(WORKSPACE_KEY, workspace);
	} catch {
		// Private mode or quota. The session still works; only reload is lost.
	}
}

/** The remembered workspace, or `null`. Never throws. */
export async function recallWorkspace(): Promise<StoredWorkspace | null> {
	const store = activeStore();
	if (!store) return null;
	try {
		const stored = await store.get(WORKSPACE_KEY);
		return isStoredWorkspace(stored) ? stored : null;
	} catch {
		return null;
	}
}

/** Forget the remembered workspace. Never throws. */
export async function forgetWorkspace(): Promise<void> {
	const store = activeStore();
	if (!store) return;
	try {
		await store.del(WORKSPACE_KEY);
	} catch {
		// Nothing to do; the caller is already falling back.
	}
}

/**
 * What comes back out of storage is checked rather than trusted: the store is
 * shared with earlier versions of this page, and a shape that no longer matches
 * should read as "no workspace" rather than crash the editor on startup.
 */
function isStoredWorkspace(value: unknown): value is StoredWorkspace {
	if (typeof value !== 'object' || value === null) return false;
	const candidate = value as { name?: unknown; files?: unknown };
	return (
		typeof candidate.name === 'string' &&
		typeof candidate.files === 'object' &&
		candidate.files !== null
	);
}
