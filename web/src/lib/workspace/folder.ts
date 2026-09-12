/**
 * The real-directory workspace, built on the File System Access API.
 *
 * The two sharp edges here are permissions and testability, and they pull in the
 * same direction. Permissions because a browser drops write access on reload
 * even when the handle was remembered, so a write can fail for a reason the user
 * must be told about rather than a reason to crash. Testability because the
 * native picker cannot be clicked by an automated browser, so every operation
 * works against the structural handles in `handles.ts` — the browser's own
 * classes satisfy them, and so does a fake tree.
 */

import {
	isPermissionError,
	readHandle,
	writeHandle,
	type DirectoryHandleLike,
	type FileHandleLike
} from './handles';
import { basename, buildTree, dirname, normalizePath, shouldSkip } from './paths';
import { hasDirectoryImport } from './import';
import { forgetHandle, recallHandle, rememberHandle } from './persistence';
import { isAbortError, resolvePicker, type DirectoryPicker } from './picker';
import type { Workspace, WorkspaceCapabilities, WorkspaceEntry, WorkspacePath } from './types';

/**
 * Thrown instead of a bare `NotAllowedError` so the UI can recognise the one
 * failure it can fix — a permission prompt the user must answer — and offer a
 * reconnect rather than reporting an edit as lost.
 */
export class WorkspacePermissionError extends Error {
	constructor(path?: WorkspacePath) {
		super(
			path
				? `Permission to write ${path} has lapsed. Reconnect the folder to continue.`
				: 'Permission to write the folder has lapsed. Reconnect it to continue.'
		);
		this.name = 'WorkspacePermissionError';
	}
}

export class FolderWorkspace implements Workspace {
	readonly kind = 'folder' as const;
	readonly name: string;

	#root: DirectoryHandleLike;
	/**
	 * Cached because `capabilities` is synchronous while the permission API is
	 * not. It is re-queried before every operation, so a grant revoked
	 * mid-session is noticed on the next edit rather than at the next reload.
	 */
	#permission: PermissionState = 'prompt';
	#files = new Map<WorkspacePath, FileHandleLike>();
	/** Includes the root under `''`, which makes `#ensureDirectory` recurse to a stop. */
	#directories = new Map<WorkspacePath, DirectoryHandleLike>();

	private constructor(handle: DirectoryHandleLike) {
		this.#root = handle;
		this.name = handle.name;
		this.#directories.set('', handle);
	}

	/**
	 * Wrap an already-obtained handle and read its tree.
	 *
	 * Never throws because permission is missing: a remembered handle whose
	 * permission lapsed after a reload must still yield a workspace, so the UI
	 * can offer to reconnect. It throws only for a handle that is genuinely gone
	 * (the directory was moved or deleted), which the caller discards.
	 */
	static async open(handle: DirectoryHandleLike): Promise<FolderWorkspace> {
		const workspace = new FolderWorkspace(handle);
		await workspace.#initialize();
		return workspace;
	}

	get capabilities(): WorkspaceCapabilities {
		return {
			canOpenFolder: true,
			// The environment's, not this backend's: an imported folder and an
			// opened one are different things, and a caller asking what it can do
			// needs both answers even while holding one of them.
			canImportFolder: hasDirectoryImport(),
			canWrite: true,
			needsPermission: this.#permission !== 'granted'
		};
	}

	entries(): WorkspaceEntry[] {
		return buildTree(this.#files.keys(), this.#directories.keys());
	}

	async read(path: WorkspacePath): Promise<string> {
		const key = normalizePath(path);
		await this.#ensurePermission(key);
		return readHandle(this.#requireFile(key));
	}

	async write(path: WorkspacePath, text: string): Promise<void> {
		const key = normalizePath(path);
		await this.#ensurePermission(key);
		let handle = this.#files.get(key);
		if (!handle) {
			// Saving a path the folder has never seen is a legitimate "save as":
			// make the file and any missing parent directories.
			const directory = await this.#ensureDirectory(dirname(key));
			handle = await directory.getFileHandle(basename(key), { create: true });
			this.#files.set(key, handle);
		}
		await writeHandle(handle, text);
	}

	async create(path: WorkspacePath, text = ''): Promise<void> {
		const key = normalizePath(path);
		await this.#ensurePermission(key);
		if (this.#files.has(key) || this.#directories.has(key)) {
			throw new Error(`Already exists: ${key}`);
		}
		const directory = await this.#ensureDirectory(dirname(key));
		const handle = await directory.getFileHandle(basename(key), { create: true });
		this.#files.set(key, handle);
		if (text !== '') await writeHandle(handle, text);
	}

	async remove(path: WorkspacePath): Promise<void> {
		const key = normalizePath(path);
		await this.#ensurePermission(key);

		if (this.#files.has(key)) {
			await this.#parentDirectory(key).removeEntry(basename(key));
			this.#files.delete(key);
			return;
		}

		if (key !== '' && this.#directories.has(key)) {
			// Directories are removed recursively because the workspace tree is
			// the user's whole view; leaving orphaned files behind would make the
			// next open disagree with what they saw disappear.
			await this.#parentDirectory(key).removeEntry(basename(key), { recursive: true });
			this.#forgetSubtree(key);
			return;
		}

		throw new Error(`No such entry: ${key}`);
	}

	async rename(from: WorkspacePath, to: WorkspacePath): Promise<void> {
		const source = normalizePath(from);
		const target = normalizePath(to);
		await this.#ensurePermission(source);
		if (source === target) return;
		if (target.startsWith(`${source}/`)) {
			throw new Error(`Cannot move ${source} inside itself.`);
		}
		if (this.#files.has(target) || this.#directories.has(target)) {
			throw new Error(`Already exists: ${target}`);
		}

		const file = this.#files.get(source);
		if (file) {
			// There is no portable `move` on file handles, so rename is copy and
			// delete. `create` also makes any missing destination directories.
			const text = await readHandle(file);
			await this.create(target, text);
			await this.remove(source);
			return;
		}

		if (this.#directories.has(source) && source !== '') {
			await this.#copyDirectory(source, target);
			await this.remove(source);
			return;
		}

		throw new Error(`No such entry: ${source}`);
	}

	async snapshot(): Promise<Record<WorkspacePath, string>> {
		await this.#ensurePermission();
		const files: Record<WorkspacePath, string> = {};
		// Sorted so a snapshot is byte-identical between runs, which matters when
		// it is handed to the wasm side or compared in a test.
		for (const path of [...this.#files.keys()].sort()) {
			files[path] = await readHandle(this.#files.get(path)!);
		}
		return files;
	}

	/**
	 * Ask the browser to grant write access again.
	 *
	 * This must be called from a user gesture — a "Reconnect" button — because
	 * browsers reject `requestPermission` otherwise. It is deliberately not
	 * called automatically from `write`: a prompt raised without a gesture is
	 * ignored, and failing silently is exactly the behaviour to avoid.
	 */
	async reconnect(): Promise<boolean> {
		const request = this.#root.requestPermission;
		if (typeof request !== 'function') {
			this.#permission = await this.#queryPermission();
		} else {
			try {
				this.#permission = await request.call(this.#root, { mode: 'readwrite' });
			} catch {
				this.#permission = 'denied';
			}
		}
		if (this.#permission === 'granted') await this.#reload();
		return this.#permission === 'granted';
	}

	async #initialize(): Promise<void> {
		this.#permission = await this.#queryPermission();
		if (this.#permission !== 'granted') return;
		try {
			await this.#reload();
		} catch (error) {
			if (!isPermissionError(error)) throw error;
			// The query promised access the read did not get; treat it as a
			// reconnect case rather than a reason to lose the remembered handle.
			this.#permission = 'prompt';
		}
	}

	async #queryPermission(): Promise<PermissionState> {
		const query = this.#root.queryPermission;
		// A handle without the permission API is assumed granted: that is the
		// state of a freshly picked folder, and of every fake in the tests.
		if (typeof query !== 'function') return 'granted';
		try {
			return await query.call(this.#root, { mode: 'readwrite' });
		} catch {
			return 'prompt';
		}
	}

	async #ensurePermission(path?: WorkspacePath): Promise<void> {
		this.#permission = await this.#queryPermission();
		if (this.#permission !== 'granted') throw new WorkspacePermissionError(path);
	}

	/** Walk the whole tree, skipping build output and dotfiles. */
	async #reload(): Promise<void> {
		const files = new Map<WorkspacePath, FileHandleLike>();
		const directories = new Map<WorkspacePath, DirectoryHandleLike>();
		directories.set('', this.#root);

		const walk = async (directory: DirectoryHandleLike, prefix: string): Promise<void> => {
			for await (const child of directory.values()) {
				if (shouldSkip(child.name)) continue;
				const path = prefix === '' ? child.name : `${prefix}/${child.name}`;
				if (child.kind === 'directory') {
					directories.set(path, child);
					await walk(child, path);
				} else {
					files.set(path, child);
				}
			}
		};
		await walk(this.#root, '');

		this.#files = files;
		this.#directories = directories;
	}

	#requireFile(path: string): FileHandleLike {
		const handle = this.#files.get(path);
		if (!handle) throw new Error(`No such file: ${path}`);
		return handle;
	}

	#parentDirectory(path: string): DirectoryHandleLike {
		const parent = this.#directories.get(dirname(path));
		if (!parent) throw new Error(`No such directory: ${dirname(path)}`);
		return parent;
	}

	async #ensureDirectory(path: string): Promise<DirectoryHandleLike> {
		const existing = this.#directories.get(path);
		if (existing) return existing;
		const parent = await this.#ensureDirectory(dirname(path));
		const handle = await parent.getDirectoryHandle(basename(path), { create: true });
		this.#directories.set(path, handle);
		return handle;
	}

	async #copyDirectory(source: string, target: string): Promise<void> {
		const boundary = `${source}/`;
		// Snapshot the file list before writing: the loop creates new entries in
		// `#files`, and mutating a Map while iterating it skips or repeats.
		const entries = [...this.#files.entries()].filter(([path]) => path.startsWith(boundary));
		await this.#ensureDirectory(target);
		for (const [path, handle] of entries) {
			const destination = `${target}/${path.slice(boundary.length)}`;
			const text = await readHandle(handle);
			const directory = await this.#ensureDirectory(dirname(destination));
			const created = await directory.getFileHandle(basename(destination), { create: true });
			await writeHandle(created, text);
			this.#files.set(destination, created);
		}
		// Empty directories carry no files, so they need copying explicitly.
		// Snapshot the keys: `#ensureDirectory` adds entries while we iterate.
		for (const path of [...this.#directories.keys()]) {
			if (path !== '' && path.startsWith(boundary)) {
				await this.#ensureDirectory(`${target}/${path.slice(boundary.length)}`);
			}
		}
	}

	#forgetSubtree(prefix: string): void {
		const boundary = `${prefix}/`;
		for (const path of [...this.#files.keys()]) {
			if (path.startsWith(boundary)) this.#files.delete(path);
		}
		for (const path of [...this.#directories.keys()]) {
			if (path !== '' && (path === prefix || path.startsWith(boundary))) {
				this.#directories.delete(path);
			}
		}
	}
}

/**
 * Ask the user for a folder and open it.
 *
 * Returns `null` when the picker is dismissed, because cancelling is a normal
 * outcome and not an error for the caller to report. The picker is a parameter
 * so a test can substitute one for the native dialog.
 */
export async function openFolder(picker?: DirectoryPicker): Promise<FolderWorkspace | null> {
	const choose = picker ?? resolvePicker();
	let handle: DirectoryHandleLike;
	try {
		handle = await choose({ mode: 'readwrite' });
	} catch (error) {
		if (isAbortError(error)) return null;
		throw error;
	}

	const workspace = await FolderWorkspace.open(handle);
	// Only remembered after it opened cleanly, so a bad pick does not overwrite
	// a good handle from a previous session.
	await rememberHandle(handle);
	return workspace;
}

/**
 * Re-open the folder remembered from a previous session, or `null` if there is
 * none.
 *
 * This runs during startup, so it must not throw: permission that lapsed yields
 * a workspace that reports `needsPermission`, and a handle that is genuinely
 * stale is forgotten and reported as "no folder" rather than crashing the page.
 */
export async function restore(): Promise<FolderWorkspace | null> {
	const handle = await recallHandle();
	if (!isDirectoryHandle(handle)) return null;
	try {
		return await FolderWorkspace.open(handle);
	} catch {
		await forgetHandle();
		return null;
	}
}

/** Whether a value recalled from storage still looks like a directory handle. */
function isDirectoryHandle(value: unknown): value is DirectoryHandleLike {
	if (typeof value !== 'object' || value === null) return false;
	const candidate = value as Partial<DirectoryHandleLike>;
	return (
		candidate.kind === 'directory' &&
		typeof candidate.values === 'function' &&
		typeof candidate.getFileHandle === 'function' &&
		typeof candidate.getDirectoryHandle === 'function'
	);
}
