/**
 * Tests for the parts that make the editor work outside Chromium.
 *
 * Firefox has no File System Access API, so the folder picker is not an option
 * there and the browser has to be able to (a) import a folder read-only through
 * `webkitdirectory`, (b) keep the edits somewhere, and (c) hand them back. None
 * of that can be exercised against a real dialog, which is precisely why the
 * pieces are built to be driven directly.
 */
import { describe, test } from 'bun:test';
import assert from 'node:assert/strict';
import { strFromU8, unzipSync } from 'fflate';

// Note the imports are of the modules, not of `$lib/workspace`: the barrel also
// re-exports the rune store, and `bun test` has no Svelte compiler to turn
// `$state` into anything.
import { BrowserWorkspace } from '../src/lib/workspace/browser';
import { downloadWorkspace, zipWorkspace } from '../src/lib/workspace/export';
import {
	folderNameFrom,
	hasDirectoryImport,
	readDirectoryFiles
} from '../src/lib/workspace/import';
import {
	recallWorkspace,
	rememberWorkspace,
	setHandleStore,
	type HandleStore
} from '../src/lib/workspace/persistence';

/** A `File`-like object, which is all `readDirectoryFiles` needs. */
function file(webkitRelativePath: string, text: string) {
	return {
		name: webkitRelativePath.split('/').pop() ?? webkitRelativePath,
		webkitRelativePath,
		text: async () => text
	};
}

/** A store with no IndexedDB behind it, standing in for the browser's. */
function memoryStore(): HandleStore & { map: Map<string, unknown> } {
	const map = new Map<string, unknown>();
	return {
		map,
		get: async (key) => map.get(key),
		set: async (key, value) => void map.set(key, value),
		del: async (key) => void map.delete(key)
	};
}

describe('importing a folder without the File System Access API', () => {
	test('is not claimed during server rendering', () => {
		// Every route here is prerendered, so this runs in Node with no DOM and
		// must answer rather than throw.
		assert.equal(typeof document, 'undefined');
		assert.equal(hasDirectoryImport(), false);
	});

	test('takes the folder name from the first path segment', () => {
		assert.equal(
			folderNameFrom([file('my-project/main.fxc', ''), file('my-project/lib/x.fxc', '')]),
			'my-project'
		);
		assert.equal(folderNameFrom([file('main.fxc', '')]), 'Imported');
	});

	test('strips the folder itself, so includes resolve against the root', () => {
		// `webkitRelativePath` starts with the chosen folder. If that segment
		// survived, the entry file would be `my-project/main.fxc` and
		// `#include "lib/pack.fxc"` would resolve one level too deep.
		return readDirectoryFiles([
			file('my-project/main.fxc', 'fn main() {}'),
			file('my-project/lib/pack.fxc', 'fn pack(x, y) = x + y * i();')
		]).then((files) => {
			assert.deepEqual(Object.keys(files).sort(), ['lib/pack.fxc', 'main.fxc']);
			assert.equal(files['lib/pack.fxc'], 'fn pack(x, y) = x + y * i();');
		});
	});

	test('drops what should never enter a workspace', async () => {
		const files = await readDirectoryFiles([
			file('proj/main.fxc', 'kept'),
			file('proj/node_modules/dep/index.js', 'dropped'),
			file('proj/.git/config', 'dropped'),
			file('proj/target/debug/thing', 'dropped'),
			file('proj/.hidden', 'dropped'),
			file('proj/lib/.dotfile', 'dropped')
		]);
		assert.deepEqual(Object.keys(files), ['main.fxc']);
	});
});

describe('exporting', () => {
	test('zips the workspace so it unpacks to the same tree', () => {
		const files = { 'main.fxc': 'fn main() {}', 'lib/pack.fxc': 'fn pack() = 1;' };
		const unzipped = unzipSync(zipWorkspace(files));
		assert.deepEqual(Object.keys(unzipped).sort(), ['lib/pack.fxc', 'main.fxc']);
		assert.equal(strFromU8(unzipped['main.fxc']), 'fn main() {}');
		assert.equal(strFromU8(unzipped['lib/pack.fxc']), 'fn pack() = 1;');
	});

	test('an empty workspace still produces a readable archive', () => {
		assert.deepEqual(Object.keys(unzipSync(zipWorkspace({}))), []);
	});

	test('handing it to the DOM is not attempted without one', () => {
		// The download itself needs a document; that it is only called from a
		// click is the caller's job, and this asserts the boundary is real.
		assert.equal(typeof document, 'undefined');
		assert.throws(() => downloadWorkspace('x', { 'a.fxc': '' }));
	});
});

describe('the browser workspace', () => {
	test('keeps edits across a "reload"', async () => {
		const store = memoryStore();
		setHandleStore(store);
		try {
			const workspace = new BrowserWorkspace({ 'main.fxc': '1' }, 'proj');
			await workspace.write('main.fxc', '2');
			await workspace.create('lib/x.fxc', '3');

			// A second instance stands in for a fresh page load.
			const restored = await BrowserWorkspace.restore();
			assert.ok(restored, 'expected the workspace to come back');
			assert.equal(restored.name, 'proj');
			assert.equal(await restored.read('main.fxc'), '2');
			assert.equal(await restored.read('lib/x.fxc'), '3');
		} finally {
			setHandleStore(null);
		}
	});

	test('reports itself as a browser workspace, not a folder', () => {
		const workspace = new BrowserWorkspace();
		assert.equal(workspace.kind, 'browser');
	});

	test('there is nothing to restore when storage is empty', async () => {
		setHandleStore(memoryStore());
		try {
			assert.equal(await BrowserWorkspace.restore(), null);
		} finally {
			setHandleStore(null);
		}
	});

	test('a malformed stored workspace reads as absent, not as a crash', async () => {
		// The store is shared with older versions of the page, so what comes back
		// is checked rather than trusted.
		const store = memoryStore();
		setHandleStore(store);
		try {
			await store.set('fx50.workspace.files', { name: 'x' });
			assert.equal(await recallWorkspace(), null);
			assert.equal(await BrowserWorkspace.restore(), null);
		} finally {
			setHandleStore(null);
		}
	});

	test('storage that throws does not take the workspace with it', async () => {
		// A private window, or a full quota. Losing the reload convenience is
		// acceptable; losing the edit is not.
		setHandleStore({
			get: async () => {
				throw new Error('blocked');
			},
			set: async () => {
				throw new Error('quota');
			},
			del: async () => {
				throw new Error('blocked');
			}
		});
		try {
			const workspace = new BrowserWorkspace({ 'main.fxc': '1' });
			await workspace.write('main.fxc', '2');
			assert.equal(await workspace.read('main.fxc'), '2');
			assert.equal(await BrowserWorkspace.restore(), null);
			await rememberWorkspace({ name: 'x', files: {} });
		} finally {
			setHandleStore(null);
		}
	});
});
