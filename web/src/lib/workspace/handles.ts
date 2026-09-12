/**
 * The handle-shaped slice of the File System Access API this layer uses.
 *
 * Deliberately structural rather than `FileSystemDirectoryHandle` itself: the
 * real picker cannot be driven by an automated test, so the folder backend is
 * written against these interfaces and handed a fake tree in its place. The
 * browser's own classes satisfy them unchanged, which is what keeps the fake
 * honest rather than a parallel implementation that can drift.
 */

/** The permission modes the workspace asks for. */
export type PermissionMode = 'read' | 'readwrite';

/** The subset of `FileSystemFileHandle`. */
export interface FileHandleLike {
	readonly kind: 'file';
	readonly name: string;
	/** May reject with `NotAllowedError` until permission is granted. */
	getFile(): Promise<FileLike>;
	createWritable(): Promise<WritableLike>;
}

/** The subset of `File`. */
export interface FileLike {
	text(): Promise<string>;
}

/** The subset of `FileSystemWritableFileStream` used to write text files. */
export interface WritableLike {
	write(data: string): Promise<void>;
	close(): Promise<void>;
}

/** The subset of `FileSystemDirectoryHandle`. */
export interface DirectoryHandleLike {
	readonly kind: 'directory';
	readonly name: string;
	/** Iterate children. Rejects with `NotAllowedError` without read permission. */
	values(): AsyncIterableIterator<FileHandleLike | DirectoryHandleLike>;
	getDirectoryHandle(name: string, options?: { create?: boolean }): Promise<DirectoryHandleLike>;
	getFileHandle(name: string, options?: { create?: boolean }): Promise<FileHandleLike>;
	removeEntry(name: string, options?: { recursive?: boolean }): Promise<void>;
	/**
	 * Optional because not every environment that can produce a handle
	 * implements the permission API; when absent the backend assumes access was
	 * granted at pick time, which is true for a fresh `showDirectoryPicker`.
	 */
	queryPermission?(descriptor?: { mode?: PermissionMode }): Promise<PermissionState>;
	requestPermission?(descriptor?: { mode?: PermissionMode }): Promise<PermissionState>;
}

/** A text-file write against a handle. */
export async function readHandle(handle: FileHandleLike): Promise<string> {
	const file = await handle.getFile();
	return file.text();
}

/** Replace a file's contents, always closing the writable so the handle can be reused. */
export async function writeHandle(handle: FileHandleLike, text: string): Promise<void> {
	const writable = await handle.createWritable();
	await writable.write(text);
	await writable.close();
}

/**
 * Whether an error means "no permission yet" rather than "this handle is gone".
 * The distinction decides whether a remembered handle is kept for a reconnect or
 * discarded as stale.
 */
export function isPermissionError(error: unknown): boolean {
	if (!(error instanceof Error)) return false;
	return error.name === 'NotAllowedError' || error.name === 'SecurityError';
}
