/**
 * Tests for the workspace layer, run under `bun test`.
 *
 * There are two backends and only one contract, so both are driven through the
 * same operations. The folder backend cannot be tested against a real picker —
 * Playwright and friends cannot click a native directory dialog — so it is
 * exercised against a fake handle tree that implements the same structural
 * interfaces the browser's own handles do. That is the whole reason those
 * interfaces exist in `handles.ts` rather than being `FileSystemDirectoryHandle`
 * directly.
 *
 * Assertions use `node:assert`, matching the existing wasm suite.
 */

import { afterEach, beforeEach, describe, test } from 'bun:test';
import assert from 'node:assert/strict';

import {
	FolderWorkspace,
	WorkspacePermissionError,
	openFolder,
	restore
} from '../src/lib/workspace/folder';
import { MemoryWorkspace } from '../src/lib/workspace/memory';
import { setDirectoryPicker } from '../src/lib/workspace/picker';
import {
	forgetHandle,
	recallHandle,
	rememberHandle,
	setHandleStore,
	type HandleStore
} from '../src/lib/workspace/persistence';
import type {
	DirectoryHandleLike,
	FileHandleLike,
	FileLike,
	WritableLike
} from '../src/lib/workspace/handles';

// ---------------------------------------------------------------------------
// A fake File System Access tree.
// ---------------------------------------------------------------------------

interface FakeFileNode {
	kind: 'file';
	name: string;
	text: string;
}

interface FakeDirectoryNode {
	kind: 'directory';
	name: string;
	children: Map<string, FakeNode>;
}

type FakeNode = FakeFileNode | FakeDirectoryNode;

function file(name: string, text = ''): FakeFileNode {
	return { kind: 'file', name, text };
}

function dir(name: string, ...children: FakeNode[]): FakeDirectoryNode {
	const node: FakeDirectoryNode = { kind: 'directory', name, children: new Map() };
	for (const child of children) node.children.set(child.name, child);
	return node;
}

function notFound(name: string): Error {
	const error = new Error(`NotFoundError: ${name}`);
	error.name = 'NotFoundError';
	return error;
}

class FakeFileHandle implements FileHandleLike {
	readonly kind = 'file' as const;
	#node: FakeFileNode;

	constructor(node: FakeFileNode) {
		this.#node = node;
	}

	get name(): string {
		return this.#node.name;
	}

	async getFile(): Promise<FileLike> {
		const node = this.#node;
		return { text: async () => node.text };
	}

	async createWritable(): Promise<WritableLike> {
		const node = this.#node;
		return {
			async write(data: string) {
				node.text = data;
			},
			async close() {}
		};
	}
}

class FakeDirectoryHandle implements DirectoryHandleLike {
	readonly kind = 'directory' as const;
	#node: FakeDirectoryNode;
	#permission: PermissionState;
	#grantOnRequest: boolean;

	constructor(node: FakeDirectoryNode, permission: PermissionState = 'granted', grant = true) {
		this.#node = node;
		this.#permission = permission;
		this.#grantOnRequest = grant;
	}

	get name(): string {
		return this.#node.name;
	}

	async *values(): AsyncIterableIterator<FileHandleLike | DirectoryHandleLike> {
		for (const child of this.#node.children.values()) {
			yield child.kind === 'directory'
				? new FakeDirectoryHandle(child, this.#permission, this.#grantOnRequest)
				: new FakeFileHandle(child);
		}
	}

	async getDirectoryHandle(
		name: string,
		options?: { create?: boolean }
	): Promise<DirectoryHandleLike> {
		const child = this.#node.children.get(name);
		if (child?.kind === 'directory') {
			return new FakeDirectoryHandle(child, this.#permission, this.#grantOnRequest);
		}
		if (child) throw new Error(`${name} is not a directory`);
		if (!options?.create) throw notFound(name);
		const created = dir(name);
		this.#node.children.set(name, created);
		return new FakeDirectoryHandle(created, this.#permission, this.#grantOnRequest);
	}

	async getFileHandle(name: string, options?: { create?: boolean }): Promise<FileHandleLike> {
		const child = this.#node.children.get(name);
		if (child?.kind === 'file') return new FakeFileHandle(child);
		if (child) throw new Error(`${name} is not a file`);
		if (!options?.create) throw notFound(name);
		const created = file(name);
		this.#node.children.set(name, created);
		return new FakeFileHandle(created);
	}

	async removeEntry(name: string, options?: { recursive?: boolean }): Promise<void> {
		const child = this.#node.children.get(name);
		if (!child) throw notFound(name);
		// The real API refuses to remove a non-empty directory without the flag.
		if (child.kind === 'directory' && !options?.recursive && child.children.size > 0) {
			throw new Error(`Directory not empty: ${name}`);
		}
		this.#node.children.delete(name);
	}

	async queryPermission(): Promise<PermissionState> {
		return this.#permission;
	}

	async requestPermission(): Promise<PermissionState> {
		if (this.#grantOnRequest) this.#permission = 'granted';
		return this.#permission;
	}
}

/** A project shaped to exercise the sort and skip rules at once. */
function project(): FakeDirectoryNode {
	return dir(
		'project',
		file('main.fxc', 'main'),
		file('.env', 'secret'),
		dir('lib', file('pack.fxc', 'pack'), file('util.fxc', 'util')),
		dir('empty'),
		dir('node_modules', file('junk.fxc', 'junk')),
		dir('.git', file('config', 'git')),
		dir('target', file('out.fxc', 'out')),
		dir('.svelte-kit', file('generated.fxc', 'generated')),
		dir('build', file('page.fxc', 'page'))
	);
}

beforeEach(() => {
	setDirectoryPicker(null);
	setHandleStore(null);
});

afterEach(() => {
	setDirectoryPicker(null);
	setHandleStore(null);
});

// ---------------------------------------------------------------------------
// Memory backend
// ---------------------------------------------------------------------------

describe('MemoryWorkspace', () => {
	test('reads, writes, creates, renames, removes and snapshots', async () => {
		const workspace = new MemoryWorkspace({ 'main.fxc': 'main' });
		assert.equal(workspace.kind, 'memory');
		assert.equal(await workspace.read('main.fxc'), 'main');

		await workspace.write('main.fxc', 'changed');
		assert.equal(await workspace.read('main.fxc'), 'changed');

		await workspace.create('lib/pack.fxc', 'pack');
		assert.equal(await workspace.read('lib/pack.fxc'), 'pack');

		await workspace.rename('lib/pack.fxc', 'vendor/pack.fxc');
		assert.equal(await workspace.read('vendor/pack.fxc'), 'pack');
		await assert.rejects(() => workspace.read('lib/pack.fxc'));

		await workspace.create('a.fxc');
		assert.equal(await workspace.read('a.fxc'), '');

		// `lib` is empty after the rename, so repopulate it before removing the
		// directory: memory workspaces only know directories implied by files.
		await workspace.create('lib/other.fxc', 'other');
		await workspace.remove('lib');
		await assert.rejects(() => workspace.read('lib/other.fxc'));

		await workspace.remove('vendor/pack.fxc');
		await assert.rejects(() => workspace.read('vendor/pack.fxc'));

		assert.deepEqual(await workspace.snapshot(), { 'main.fxc': 'changed', 'a.fxc': '' });
	});

	test('refuses to create or rename onto an existing path', async () => {
		const workspace = new MemoryWorkspace({ 'main.fxc': 'main', 'lib/pack.fxc': 'pack' });
		await assert.rejects(() => workspace.create('main.fxc'));
		await assert.rejects(() => workspace.rename('main.fxc', 'lib/pack.fxc'));
		await assert.rejects(() => workspace.rename('lib', 'lib/nested'));
		await assert.rejects(() => workspace.read('missing.fxc'));
	});

	test('sorts directories first, then files, and keeps lib/ subdirectories', async () => {
		const workspace = new MemoryWorkspace({
			'zeta.fxc': '',
			'lib/util.fxc': '',
			'main.fxc': '',
			'lib/pack.fxc': ''
		});
		const entries = workspace.entries();
		assert.deepEqual(
			entries.map((entry) => `${entry.kind}:${entry.path}`),
			['directory:lib', 'file:main.fxc', 'file:zeta.fxc']
		);
		assert.deepEqual(
			entries[0].children?.map((entry) => entry.path),
			['lib/pack.fxc', 'lib/util.fxc']
		);
	});
});

// ---------------------------------------------------------------------------
// Folder backend, against a fake handle tree
// ---------------------------------------------------------------------------

describe('FolderWorkspace', () => {
	test('reads a real-looking tree and applies the skip rules', async () => {
		const workspace = await FolderWorkspace.open(new FakeDirectoryHandle(project()));
		assert.equal(workspace.kind, 'folder');
		assert.equal(workspace.name, 'project');
		assert.equal(workspace.capabilities.needsPermission, false);

		assert.deepEqual(
			workspace.entries().map((entry) => `${entry.kind}:${entry.path}`),
			['directory:empty', 'directory:lib', 'file:main.fxc']
		);
		const lib = workspace.entries()[1];
		assert.deepEqual(
			lib.children?.map((entry) => entry.path),
			['lib/pack.fxc', 'lib/util.fxc']
		);

		// Dotfiles and build output never reach the tree.
		const paths = workspace.entries().map((entry) => entry.path);
		for (const skipped of ['node_modules', 'target', 'build', '.git', '.svelte-kit']) {
			assert.ok(!paths.includes(skipped), `${skipped} should be skipped`);
		}
		assert.ok(!paths.includes('.env'));
	});

	test('reads and writes in place, including lib/ subdirectories', async () => {
		const root = project();
		const workspace = await FolderWorkspace.open(new FakeDirectoryHandle(root));

		assert.equal(await workspace.read('main.fxc'), 'main');
		await workspace.write('main.fxc', 'edited');
		assert.equal(await workspace.read('main.fxc'), 'edited');

		// The write went to the underlying tree, not just an internal copy.
		const mainNode = root.children.get('main.fxc');
		assert.ok(mainNode?.kind === 'file');
		assert.equal(mainNode.text, 'edited');

		assert.equal(await workspace.read('lib/pack.fxc'), 'pack');
		await workspace.write('lib/pack.fxc', 'pack-edited');
		assert.equal(await workspace.read('lib/pack.fxc'), 'pack-edited');
	});

	test('creates missing intermediate directories', async () => {
		const workspace = await FolderWorkspace.open(new FakeDirectoryHandle(project()));
		await workspace.create('lib/deep/nested/util.fxc', 'deep');
		assert.equal(await workspace.read('lib/deep/nested/util.fxc'), 'deep');

		const lib = workspace.entries().find((entry) => entry.path === 'lib');
		assert.ok(lib?.children?.some((entry) => entry.path === 'lib/deep'));
	});

	test('renames files and directories, creating intermediate directories', async () => {
		const workspace = await FolderWorkspace.open(new FakeDirectoryHandle(project()));

		await workspace.rename('main.fxc', 'src/main.fxc');
		assert.equal(await workspace.read('src/main.fxc'), 'main');
		await assert.rejects(() => workspace.read('main.fxc'));

		await workspace.rename('lib', 'vendor/lib');
		assert.equal(await workspace.read('vendor/lib/pack.fxc'), 'pack');
		assert.equal(await workspace.read('vendor/lib/util.fxc'), 'util');
		await assert.rejects(() => workspace.read('lib/pack.fxc'));

		// The source directory is gone from the tree, not just its files.
		const paths = workspace.entries().map((entry) => entry.path);
		assert.ok(!paths.includes('lib'));
		assert.ok(paths.includes('vendor'));
	});

	test('removes files and directories', async () => {
		const workspace = await FolderWorkspace.open(new FakeDirectoryHandle(project()));

		await workspace.remove('main.fxc');
		await assert.rejects(() => workspace.read('main.fxc'));

		await workspace.remove('lib');
		await assert.rejects(() => workspace.read('lib/pack.fxc'));
		assert.equal(
			workspace.entries().some((entry) => entry.path === 'lib'),
			false
		);
	});

	test('snapshots every text file keyed by relative path', async () => {
		const workspace = await FolderWorkspace.open(new FakeDirectoryHandle(project()));
		assert.deepEqual(await workspace.snapshot(), {
			'main.fxc': 'main',
			'lib/pack.fxc': 'pack',
			'lib/util.fxc': 'util'
		});
	});

	test('reports needsPermission instead of throwing when access lapsed', async () => {
		const workspace = await FolderWorkspace.open(new FakeDirectoryHandle(project(), 'prompt'));

		assert.equal(workspace.capabilities.needsPermission, true);
		assert.equal(workspace.capabilities.canWrite, true);
		assert.deepEqual(workspace.entries(), []);

		// The failure is the typed one the UI can react to, not a bare DOM error.
		await assert.rejects(
			() => workspace.write('main.fxc', 'nope'),
			(error: unknown) => error instanceof WorkspacePermissionError
		);
	});

	test('reconnect re-grants permission and reloads the tree', async () => {
		const workspace = await FolderWorkspace.open(new FakeDirectoryHandle(project(), 'prompt'));
		assert.equal(await workspace.reconnect(), true);
		assert.equal(workspace.capabilities.needsPermission, false);
		assert.ok(workspace.entries().length > 0);
		assert.equal(await workspace.read('main.fxc'), 'main');
	});

	test('refuses to create or rename onto an existing path', async () => {
		const workspace = await FolderWorkspace.open(new FakeDirectoryHandle(project()));
		await assert.rejects(() => workspace.create('main.fxc'));
		await assert.rejects(() => workspace.create('lib'));
		await assert.rejects(() => workspace.rename('main.fxc', 'lib/pack.fxc'));
	});
});

// ---------------------------------------------------------------------------
// Picker injection and restore
// ---------------------------------------------------------------------------

describe('openFolder and restore', () => {
	test('openFolder uses the injected picker and remembers the handle', async () => {
		const root = project();
		const remembered: unknown[] = [];
		setDirectoryPicker(async () => new FakeDirectoryHandle(root));
		setHandleStore({
			get: async () => undefined,
			set: async (_key, value) => {
				remembered.push(value);
			},
			del: async () => {}
		});

		const workspace = await openFolder();
		assert.ok(workspace);
		assert.equal(workspace.name, 'project');
		assert.equal(remembered.length, 1);
	});

	test('openFolder returns null when the picker is dismissed', async () => {
		setDirectoryPicker(async () => {
			const error = new Error('cancelled');
			error.name = 'AbortError';
			throw error;
		});
		assert.equal(await openFolder(), null);
	});

	test('restore returns null when nothing was remembered', async () => {
		setHandleStore({
			get: async () => undefined,
			set: async () => {},
			del: async () => {}
		});
		assert.equal(await restore(), null);
	});

	test('restore returns null when the store itself fails', async () => {
		setHandleStore({
			get: async () => {
				throw new Error('storage unavailable');
			},
			set: async () => {},
			del: async () => {}
		});
		assert.equal(await restore(), null);
	});

	test('restore returns a workspace that needs permission rather than failing', async () => {
		setHandleStore({
			get: async () => new FakeDirectoryHandle(project(), 'prompt'),
			set: async () => {},
			del: async () => {}
		});

		const workspace = await restore();
		assert.ok(workspace);
		assert.equal(workspace.capabilities.needsPermission, true);
		assert.deepEqual(workspace.entries(), []);
	});

	test('restore forgets a stale handle and returns null instead of throwing', async () => {
		const stale = new FakeDirectoryHandle(project());
		// A handle whose directory has been moved out from under it: permission
		// is fine, enumeration is not.
		Object.defineProperty(stale, 'values', {
			value: () => {
				throw notFound('project');
			}
		});
		let forgot = false;
		setHandleStore({
			get: async () => stale,
			set: async () => {},
			del: async () => {
				forgot = true;
			}
		});

		assert.equal(await restore(), null);
		assert.equal(forgot, true);
	});
});

// ---------------------------------------------------------------------------
// The persistence shim itself
// ---------------------------------------------------------------------------

describe('handle persistence', () => {
	test('a custom store is used without IndexedDB', async () => {
		const values = new Map<string, unknown>();
		const store: HandleStore = {
			get: async (key) => values.get(key),
			set: async (key, value) => {
				values.set(key, value);
			},
			del: async (key) => {
				values.delete(key);
			}
		};
		setHandleStore(store);

		await rememberHandle({ kind: 'directory' });
		assert.deepEqual(await recallHandle(), { kind: 'directory' });
		await forgetHandle();
		assert.equal(await recallHandle(), null);
	});
});
