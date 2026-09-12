/**
 * Where the directory picker comes from.
 *
 * `window.showDirectoryPicker` cannot be invoked by an automated browser, so the
 * backend never calls it directly. It asks this module for a picker, and a test
 * — or a harness that drives a fake folder — can install one with
 * `setDirectoryPicker` without the real dialog ever appearing.
 */

import type { DirectoryHandleLike, PermissionMode } from './handles';

/** Opens the native directory chooser. Rejects with `AbortError` if the user cancels. */
export type DirectoryPicker = (options?: { mode?: PermissionMode }) => Promise<DirectoryHandleLike>;

let injected: DirectoryPicker | null = null;

/** Install a picker (tests), or `null` to fall back to the browser's. */
export function setDirectoryPicker(picker: DirectoryPicker | null): void {
	injected = picker;
}

/**
 * Whether a folder can be opened here. Checked through `globalThis` rather than
 * `window` so that merely importing this module is safe outside a browser.
 */
export function hasDirectoryPicker(): boolean {
	if (injected) return true;
	const scope = globalThis as { showDirectoryPicker?: unknown };
	return typeof scope.showDirectoryPicker === 'function';
}

/**
 * The picker to call. Throwing from here rather than returning null keeps the
 * "unsupported" case close to its cause, where the caller can explain it.
 */
export function resolvePicker(): DirectoryPicker {
	if (injected) return injected;
	const scope = globalThis as {
		showDirectoryPicker?: (options?: { mode?: PermissionMode }) => Promise<DirectoryHandleLike>;
	};
	const picker = scope.showDirectoryPicker;
	if (typeof picker !== 'function') {
		throw new Error(
			'This browser has no File System Access API, so a folder cannot be opened. ' +
				'Use an in-memory workspace instead.'
		);
	}
	// Bound to `globalThis` because the native method checks its receiver.
	return (options) => picker.call(globalThis, options);
}

/** `AbortError` is the user dismissing the dialog, which is not a failure. */
export function isAbortError(error: unknown): boolean {
	return error instanceof Error && error.name === 'AbortError';
}
