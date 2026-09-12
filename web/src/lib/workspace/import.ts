/**
 * Opening a real folder without the File System Access API.
 *
 * Firefox has no `showDirectoryPicker`, so a Firefox user cannot open a folder
 * the way a Chromium user can. This is the alternative that works everywhere:
 * `<input type="file" webkitdirectory>`, which the browser fills with every file
 * beneath the directory the user picks, each `File` carrying a
 * `webkitRelativePath` describing where it sat.
 *
 * The trade is stark and worth stating: this can **read** a folder but never
 * write to it. That is why imported work is kept in browser storage and offered
 * for export rather than being saved back over the originals.
 *
 * The types here are deliberately structural — the browser's own `File`
 * satisfies them unchanged — so a test can drive the whole thing with plain
 * objects.
 */

import { shouldSkip } from './paths';

/** The parts of `File` this module needs. */
export interface DirectoryFileLike {
	name: string;
	/** Present on files from a `webkitdirectory` input; absent from a plain picker. */
	webkitRelativePath?: string;
	text(): Promise<string>;
}

/**
 * Whether this browser can import a folder.
 *
 * Feature-detected on a throwaway element rather than by user-agent string, and
 * guarded on `document` so importing this module during server rendering — every
 * route here is prerendered — does not touch the DOM.
 */
export function hasDirectoryImport(): boolean {
	if (typeof document === 'undefined') return false;
	return 'webkitdirectory' in document.createElement('input');
}

/** The chosen directory's own name, which the paths carry as their first segment. */
export function folderNameFrom(files: readonly DirectoryFileLike[]): string {
	const relative = files.find((file) => file.webkitRelativePath)?.webkitRelativePath ?? '';
	const [top, ...rest] = relative.split('/');
	// A path with no directory part means the browser handed back bare file
	// names, so there is no folder to take a name from.
	return top && rest.length > 0 ? top : 'Imported';
}

/**
 * Read every file, keyed by its path *inside* the chosen folder.
 *
 * `webkitRelativePath` starts with the folder the user picked — `my-project`,
 * not `my-project/lib/pack.fxc` → `main.fxc`. That first segment must go: the
 * workspace root *is* that folder, so the entry file is `main.fxc` and
 * `#include "lib/x.fxc"` has to resolve against it rather than against a
 * directory named after the folder.
 *
 * Names that should never enter a workspace are dropped, using the same rule as
 * the folder backend so the two cannot disagree about what a workspace holds.
 */
export async function readDirectoryFiles(
	files: readonly DirectoryFileLike[]
): Promise<Record<string, string>> {
	const out: Record<string, string> = {};
	for (const file of files) {
		const relative = file.webkitRelativePath || file.name;
		const segments = relative.split('/');
		// Drop the top-level folder segment, then anything skipped inside.
		const path = (segments.length > 1 ? segments.slice(1) : segments).join('/');
		if (!path) continue;
		if (path.split('/').some(shouldSkip)) continue;
		out[path] = await file.text();
	}
	return out;
}

/** A file input configured to pick a directory. Created on demand, never at module scope. */
function directoryInput(): HTMLInputElement {
	const input = document.createElement('input');
	input.type = 'file';
	// Not in the DOM lib on every TypeScript version, and it is the whole point.
	(input as HTMLInputElement & { webkitdirectory: boolean }).webkitdirectory = true;
	input.multiple = true;
	return input;
}

/**
 * Prompt for a folder and read it.
 *
 * Resolves to `null` when the user dismisses the dialog. Browsers signal that
 * with a `cancel` event, which not every version fires — so a dismissal may
 * simply leave this pending forever rather than settling. That is survivable
 * here because nothing is blocked on it; the UI is already showing the previous
 * workspace.
 */
export async function importFolder(): Promise<{
	name: string;
	files: Record<string, string>;
} | null> {
	const input = directoryInput();
	const chosen = await new Promise<FileList | null>((resolve) => {
		input.addEventListener('change', () => resolve(input.files), { once: true });
		input.addEventListener('cancel', () => resolve(null), { once: true });
		input.click();
	});
	if (!chosen || chosen.length === 0) return null;
	const files = [...chosen];
	return { name: folderNameFrom(files), files: await readDirectoryFiles(files) };
}
