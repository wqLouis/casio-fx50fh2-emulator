/**
 * Getting the work back out.
 *
 * A browser that can read a folder but not write to it leaves the user holding
 * edits with nowhere to put them, so the workspace can be zipped and downloaded.
 * That matters most on Firefox, where importing a folder is read-only, but the
 * button is offered wherever the workspace is not on disk.
 *
 * `fflate` does the container: writing a zip by hand is generic work with real
 * edge cases (local headers, the central directory, CRC-32), which is exactly the
 * kind of thing [ADR 0032](../../../../docs/DECISIONS.md) says to take from a
 * library. It is small, and has no dependencies of its own.
 */

import { strToU8, zipSync } from 'fflate';

/**
 * Zip a workspace. Paths become the entries, so the archive unpacks to the same
 * tree the user was editing.
 */
export function zipWorkspace(files: Record<string, string>): Uint8Array {
	const entries: Record<string, Uint8Array> = {};
	for (const [path, text] of Object.entries(files)) {
		entries[path] = strToU8(text);
	}
	return zipSync(entries);
}

/**
 * Download the workspace as a `.zip`.
 *
 * Must be called from a user gesture, or the browser blocks the download.
 */
export function downloadWorkspace(name: string, files: Record<string, string>): void {
	const blob = new Blob([zipWorkspace(files) as unknown as BlobPart], { type: 'application/zip' });
	const url = URL.createObjectURL(blob);

	const anchor = document.createElement('a');
	anchor.href = url;
	anchor.download = `${name || 'workspace'}.zip`;
	anchor.click();

	// Revoked on the next tick rather than immediately: revoking synchronously
	// can cancel the download before the browser has read the blob.
	setTimeout(() => URL.revokeObjectURL(url), 0);
}
