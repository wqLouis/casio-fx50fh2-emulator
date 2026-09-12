/**
 * The in-memory workspace.
 *
 * This is the fallback for a browser without the File System Access API, the
 * backing store for a scratch file, and the implementation the tests drive
 * end to end. It is also what the folder backend is measured against: both
 * satisfy the same contract, so a difference in behaviour is a bug in one of
 * them rather than a backend-specific quirk.
 */

import { hasDirectoryImport } from './import';
import { hasDirectoryPicker } from './picker';
import { buildTree, normalizePath } from './paths';
import type { Workspace, WorkspaceCapabilities, WorkspaceEntry, WorkspacePath } from './types';

export class MemoryWorkspace implements Workspace {
	readonly kind = 'memory' as const;
	readonly name: string;

	#files = new Map<WorkspacePath, string>();

	constructor(files?: Record<WorkspacePath, string>, name = 'Scratch') {
		for (const [path, text] of Object.entries(files ?? {})) {
			this.#files.set(normalizePath(path), text);
		}
		this.name = name;
	}

	get capabilities(): WorkspaceCapabilities {
		return {
			// Even in memory the UI may offer to open or import a real folder, so
			// these report the environment rather than the backend.
			canOpenFolder: hasDirectoryPicker(),
			canImportFolder: hasDirectoryImport(),
			canWrite: true,
			needsPermission: false
		};
	}

	entries(): WorkspaceEntry[] {
		return buildTree(this.#files.keys());
	}

	async read(path: WorkspacePath): Promise<string> {
		const key = normalizePath(path);
		const text = this.#files.get(key);
		if (text === undefined) throw new Error(`No such file: ${key}`);
		return text;
	}

	async write(path: WorkspacePath, text: string): Promise<void> {
		this.#files.set(normalizePath(path), text);
	}

	async create(path: WorkspacePath, text = ''): Promise<void> {
		const key = normalizePath(path);
		if (this.#exists(key)) throw new Error(`Already exists: ${key}`);
		this.#files.set(key, text);
	}

	async remove(path: WorkspacePath): Promise<void> {
		const key = normalizePath(path);
		if (this.#files.delete(key)) return;
		// Not a file, so it can only be a directory implied by its children.
		const children = this.#descendants(key);
		if (children.length === 0) throw new Error(`No such entry: ${key}`);
		for (const child of children) this.#files.delete(child);
	}

	async rename(from: WorkspacePath, to: WorkspacePath): Promise<void> {
		const source = normalizePath(from);
		const target = normalizePath(to);
		if (source === target) return;
		if (target.startsWith(`${source}/`)) {
			throw new Error(`Cannot move ${source} inside itself.`);
		}
		if (this.#exists(target)) throw new Error(`Already exists: ${target}`);

		const text = this.#files.get(source);
		if (text !== undefined) {
			this.#files.delete(source);
			this.#files.set(target, text);
			return;
		}

		const children = this.#descendants(source);
		if (children.length === 0) throw new Error(`No such entry: ${source}`);
		for (const child of children) {
			this.#files.set(`${target}/${child.slice(source.length + 1)}`, this.#files.get(child)!);
		}
		for (const child of children) this.#files.delete(child);
	}

	async snapshot(): Promise<Record<WorkspacePath, string>> {
		return Object.fromEntries(this.#files);
	}

	/** Whether `path` is a file or a directory implied by files beneath it. */
	#exists(path: string): boolean {
		return this.#files.has(path) || this.#descendants(path).length > 0;
	}

	/** File paths strictly beneath `path` (a directory), in insertion order. */
	#descendants(path: string): string[] {
		const prefix = `${path}/`;
		const out: string[] = [];
		for (const key of this.#files.keys()) {
			if (key.startsWith(prefix)) out.push(key);
		}
		return out;
	}
}
