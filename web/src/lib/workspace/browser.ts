/**
 * A workspace that lives in the browser.
 *
 * Two situations need this, and they are the same thing:
 *
 * * **Firefox**, which can import a folder but never write back to it. Edits
 *   have to go somewhere, so they go here.
 * * **The simple editor route**, where there is no folder at all and the whole
 *   point is that nothing is asked of the user.
 *
 * It deliberately wraps [`MemoryWorkspace`] rather than reimplementing it: the
 * file semantics are identical, and the only difference is that this one writes
 * through to storage after every change. One implementation of the tricky parts
 * (rename a directory, delete an implied directory) is better than two that can
 * drift.
 *
 * Storage failures are swallowed — see `persistence.ts`. Losing the reload
 * convenience is acceptable; throwing away the user's edit because a private
 * window has no IndexedDB is not.
 */

import { MemoryWorkspace } from './memory';
import { recallWorkspace, rememberWorkspace } from './persistence';
import type { Workspace, WorkspaceCapabilities, WorkspaceEntry, WorkspacePath } from './types';

export class BrowserWorkspace implements Workspace {
	readonly kind = 'browser' as const;

	readonly #inner: MemoryWorkspace;

	constructor(files?: Record<WorkspacePath, string>, name = 'Scratch') {
		this.#inner = new MemoryWorkspace(files, name);
	}

	/** The workspace this browser is holding, or `null` if there is not one. */
	static async restore(): Promise<BrowserWorkspace | null> {
		const stored = await recallWorkspace();
		if (!stored) return null;
		return new BrowserWorkspace(stored.files, stored.name);
	}

	get name(): string {
		return this.#inner.name;
	}

	get capabilities(): WorkspaceCapabilities {
		return this.#inner.capabilities;
	}

	entries(): WorkspaceEntry[] {
		return this.#inner.entries();
	}

	read(path: WorkspacePath): Promise<string> {
		return this.#inner.read(path);
	}

	async write(path: WorkspacePath, text: string): Promise<void> {
		await this.#inner.write(path, text);
		await this.persist();
	}

	async create(path: WorkspacePath, text?: string): Promise<void> {
		await this.#inner.create(path, text);
		await this.persist();
	}

	async remove(path: WorkspacePath): Promise<void> {
		await this.#inner.remove(path);
		await this.persist();
	}

	async rename(from: WorkspacePath, to: WorkspacePath): Promise<void> {
		await this.#inner.rename(from, to);
		await this.persist();
	}

	snapshot(): Promise<Record<WorkspacePath, string>> {
		return this.#inner.snapshot();
	}

	/**
	 * Write the whole workspace through to storage.
	 *
	 * Public because adopting a workspace — after an import, say — has to seed
	 * storage before anything mutates it.
	 */
	async persist(): Promise<void> {
		await rememberWorkspace({ name: this.name, files: await this.#inner.snapshot() });
	}
}
