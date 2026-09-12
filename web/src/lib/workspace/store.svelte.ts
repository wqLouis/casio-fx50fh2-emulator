/**
 * The workspace singleton the UI binds to.
 *
 * This file exists for one reason: Svelte 5 runes only compile in `.svelte.ts`
 * modules, and the store must be reactive. Everything substantive lives behind
 * the `Workspace` interface in `folder.ts` and `memory.ts`; this is the thin,
 * observable shell that tracks which backend is open and mirrors its state into
 * `$state` fields a component can read.
 */

import { BrowserWorkspace } from './browser';
import { downloadWorkspace } from './export';
import {
	FolderWorkspace,
	WorkspacePermissionError,
	openFolder,
	restore as restoreFolder
} from './folder';
import { hasDirectoryImport, importFolder as promptForFolder } from './import';
import { MemoryWorkspace } from './memory';
import { hasDirectoryPicker } from './picker';
import type {
	Workspace,
	WorkspaceCapabilities,
	WorkspaceEntry,
	WorkspaceKind,
	WorkspacePath
} from './types';

export type WorkspaceStatus = 'empty' | 'ready' | 'loading' | 'error';

class WorkspaceStore {
	/** `null` until a backend is opened; the UI shows a start screen then. */
	kind = $state<WorkspaceKind | null>(null);
	name = $state('');
	capabilities = $state<WorkspaceCapabilities>({
		// Even with nothing open the UI can say which routes to a folder exist:
		// a Chromium browser can open one for reading *and* writing, a Firefox
		// one can only import it, and both are worth offering rather than
		// showing a button that does nothing.
		canOpenFolder: hasDirectoryPicker(),
		canImportFolder: hasDirectoryImport(),
		canWrite: false,
		needsPermission: false
	});
	entries = $state<WorkspaceEntry[]>([]);
	status = $state<WorkspaceStatus>('empty');
	error = $state<string | null>(null);

	/**
	 * Not reactive: the backend is an implementation detail. Callers read state
	 * through the mirrored fields and act through the methods, so a component
	 * never has to know whether it is talking to a folder or a map.
	 */
	#backend: Workspace | null = null;

	/** Open a real folder. Returns `false` if the user dismissed the picker. */
	async openFolder(): Promise<boolean> {
		this.status = 'loading';
		this.error = null;
		try {
			const folder = await openFolder();
			if (!folder) {
				// Cancelling leaves whatever was open untouched.
				this.status = this.#backend ? 'ready' : 'empty';
				return false;
			}
			this.#adopt(folder);
			return true;
		} catch (cause) {
			this.#fail(cause);
			return false;
		}
	}

	/**
	 * Start (or replace the workspace with) an in-memory one.
	 *
	 * Discarded when the page goes away — this is the throwaway case, not the
	 * one to reach for in the UI. A workspace the user should keep is
	 * [`openBrowser`], which persists.
	 */
	openMemory(files?: Record<WorkspacePath, string>, name?: string): void {
		this.#adopt(new MemoryWorkspace(files, name));
	}

	/**
	 * Start a workspace kept in this browser, restoring the last one if there is
	 * one. This is what a Firefox user gets, and what the simple editor uses.
	 */
	async openBrowser(
		files?: Record<WorkspacePath, string>,
		name?: string
	): Promise<BrowserWorkspace> {
		const backend = new BrowserWorkspace(files, name);
		this.#adopt(backend);
		// Seed storage immediately, so a reload before the first edit still finds
		// the workspace rather than an empty one.
		await backend.persist();
		return backend;
	}

	/**
	 * Ask for a folder and import it read-only.
	 *
	 * The File System Access API's picker cannot be the only route in, because
	 * Firefox has no such API; this is the one that works there. It reads the
	 * folder but can never write back, so the result is a browser workspace and
	 * the UI offers an export.
	 *
	 * Returns `false` when the user dismisses the dialog.
	 */
	async importFolder(): Promise<boolean> {
		this.status = 'loading';
		this.error = null;
		try {
			const imported = await promptForFolder();
			if (!imported) {
				this.status = this.#backend ? 'ready' : 'empty';
				return false;
			}
			await this.openBrowser(imported.files, imported.name);
			return true;
		} catch (cause) {
			this.#fail(cause);
			return false;
		}
	}

	/**
	 * Download the workspace as a `.zip`.
	 *
	 * Called from a click. It exists because a read-only import leaves the user's
	 * edits with nowhere to go; on a real folder it is redundant but harmless.
	 */
	async exportWorkspace(): Promise<void> {
		const files = await this.snapshot();
		downloadWorkspace(this.name || 'workspace', files);
	}

	/**
	 * Re-open whatever workspace was open last, preferring a real folder.
	 *
	 * A remembered folder whose permission has lapsed still comes back, with
	 * `needsPermission` set; the browser workspace is the fallback rather than
	 * the alternative, so a Firefox user's imported work reappears too.
	 */
	async restore(): Promise<boolean> {
		this.status = 'loading';
		this.error = null;
		try {
			const folder = await restoreFolder();
			if (folder) {
				this.#adopt(folder);
				return true;
			}
			const browser = await BrowserWorkspace.restore();
			if (browser) {
				this.#adopt(browser);
				return true;
			}
			this.status = this.#backend ? 'ready' : 'empty';
			return false;
		} catch (cause) {
			this.#fail(cause);
			return false;
		}
	}

	/**
	 * Re-grant folder permission. Must be wired to a click so the browser
	 * accepts the request; see `FolderWorkspace.reconnect`.
	 */
	async reconnect(): Promise<boolean> {
		if (!(this.#backend instanceof FolderWorkspace)) return false;
		try {
			const granted = await this.#backend.reconnect();
			this.#sync();
			return granted;
		} catch (cause) {
			this.#fail(cause);
			return false;
		}
	}

	read(path: WorkspacePath): Promise<string> {
		return this.#mutate((backend) => backend.read(path));
	}

	write(path: WorkspacePath, text: string): Promise<void> {
		return this.#mutate((backend) => backend.write(path, text));
	}

	create(path: WorkspacePath, text?: string): Promise<void> {
		return this.#mutate((backend) => backend.create(path, text));
	}

	remove(path: WorkspacePath): Promise<void> {
		return this.#mutate((backend) => backend.remove(path));
	}

	rename(from: WorkspacePath, to: WorkspacePath): Promise<void> {
		return this.#mutate((backend) => backend.rename(from, to));
	}

	/** Every text file keyed by relative path — the `files` map the wasm API takes. */
	snapshot(): Promise<Record<WorkspacePath, string>> {
		return this.#mutate((backend) => backend.snapshot());
	}

	#adopt(backend: Workspace): void {
		this.#backend = backend;
		this.kind = backend.kind;
		this.name = backend.name;
		this.status = 'ready';
		this.error = null;
		this.#sync();
	}

	/** Mirror the backend's current tree and capabilities into reactive state. */
	#sync(): void {
		if (!this.#backend) return;
		this.entries = this.#backend.entries();
		this.capabilities = this.#backend.capabilities;
	}

	async #mutate<T>(operation: (backend: Workspace) => Promise<T>): Promise<T> {
		const backend = this.#backend;
		if (!backend) throw new Error('No workspace is open.');
		try {
			const result = await operation(backend);
			this.#sync();
			return result;
		} catch (cause) {
			this.#fail(cause);
			throw cause;
		}
	}

	#fail(cause: unknown): void {
		this.error = cause instanceof Error ? cause.message : String(cause);
		// A lapsed permission is not a broken workspace: nothing was attempted
		// and lost, it just needs reconnecting. Keeping the status `ready` lets
		// the reconnect affordance, driven by `capabilities`, do its job.
		if (cause instanceof WorkspacePermissionError) {
			this.status = 'ready';
			if (this.#backend) this.capabilities = this.#backend.capabilities;
			return;
		}
		this.status = 'error';
	}
}

export const workspace = new WorkspaceStore();
