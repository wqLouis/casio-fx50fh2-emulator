/**
 * Path and tree arithmetic shared by both workspace backends.
 *
 * The two backends disagree about almost everything — one is a map, the other a
 * directory on disk — but they must agree on what a path *means* and on how the
 * tree is ordered. Keeping that in one place is what makes `entries()` a
 * contract rather than two implementations that drift.
 */

import type { WorkspaceEntry, WorkspacePath } from './types';

/**
 * Directory names that are build output, dependency caches or editor plumbing,
 * never part of a `.fxc` project. They are skipped so that opening a real
 * project does not drag in tens of thousands of files that `#include` will never
 * reference, and so the tree stays something a person can read.
 *
 * Dot-entries are skipped by the `startsWith('.')` rule below; `.git` and
 * `.svelte-kit` are listed anyway because the reason they are skipped is
 * specific and worth stating, not merely an accident of their name.
 */
export const SKIPPED_NAMES: ReadonlySet<string> = new Set([
	'node_modules',
	'.git',
	'target',
	'.svelte-kit',
	'build'
]);

/** Whether a child of a directory should be hidden from the workspace tree. */
export function shouldSkip(name: string): boolean {
	return name.startsWith('.') || SKIPPED_NAMES.has(name);
}

/**
 * Canonicalise a workspace-relative path.
 *
 * Backslashes are not translated: workspace paths are POSIX-style by contract,
 * and silently accepting `lib\pack.fxc` would make two spellings of one file.
 * `..` is rejected when it would leave the root — the workspace is a sandbox and
 * a path that escapes it is a bug in the caller, not something to clamp.
 */
export function normalizePath(path: WorkspacePath): string {
	const segments = path.split('/');
	const out: string[] = [];
	for (const segment of segments) {
		if (segment === '' || segment === '.') continue;
		if (segment === '..') {
			if (out.length === 0) {
				throw new Error(`Path escapes the workspace root: ${JSON.stringify(path)}`);
			}
			out.pop();
			continue;
		}
		out.push(segment);
	}
	if (out.length === 0) {
		throw new Error(`Empty workspace path: ${JSON.stringify(path)}`);
	}
	return out.join('/');
}

/** The path's parent directory, or `''` for a top-level entry. */
export function dirname(path: string): string {
	const at = path.lastIndexOf('/');
	return at === -1 ? '' : path.slice(0, at);
}

/** The path's final segment. */
export function basename(path: string): string {
	const at = path.lastIndexOf('/');
	return at === -1 ? path : path.slice(at + 1);
}

/**
 * The tree ordering the contract promises: directories before files, then
 * alphabetically. Plain code-point comparison rather than `localeCompare`, so
 * the order does not change with the host's locale.
 */
export function compareEntries(a: WorkspaceEntry, b: WorkspaceEntry): number {
	if (a.kind !== b.kind) return a.kind === 'directory' ? -1 : 1;
	return a.name < b.name ? -1 : a.name > b.name ? 1 : 0;
}

/** Sort a tree in place, depth first. */
export function sortTree(entries: WorkspaceEntry[]): WorkspaceEntry[] {
	entries.sort(compareEntries);
	for (const entry of entries) {
		if (entry.children) sortTree(entry.children);
	}
	return entries;
}

/**
 * Build a sorted tree from flat path sets.
 *
 * Directories are explicit as well as implied so that an empty directory — one
 * the user made but has not put a file in yet — still appears. A directory with
 * no children and no explicit entry would be invisible if the tree were derived
 * from file paths alone.
 */
export function buildTree(
	filePaths: Iterable<string>,
	directoryPaths: Iterable<string> = []
): WorkspaceEntry[] {
	const root: WorkspaceEntry[] = [];
	const directories = new Map<string, WorkspaceEntry>();

	const ensureDirectory = (path: string): WorkspaceEntry => {
		const existing = directories.get(path);
		if (existing) return existing;
		const entry: WorkspaceEntry = {
			path,
			name: basename(path),
			kind: 'directory',
			children: []
		};
		directories.set(path, entry);
		const parent = dirname(path);
		if (parent === '') root.push(entry);
		else ensureDirectory(parent).children!.push(entry);
		return entry;
	};

	for (const path of directoryPaths) {
		if (path !== '') ensureDirectory(path);
	}

	for (const path of filePaths) {
		const entry: WorkspaceEntry = { path, name: basename(path), kind: 'file' };
		const parent = dirname(path);
		if (parent === '') root.push(entry);
		else ensureDirectory(parent).children!.push(entry);
	}

	return sortTree(root);
}
