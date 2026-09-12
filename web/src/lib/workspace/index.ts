/**
 * The public surface of `$lib/workspace`.
 *
 * Routes and components import `workspace` and never the backends; the backends
 * are exported anyway because a test — or a future headless tool — needs to
 * drive them directly.
 */

export * from './types';
export { MemoryWorkspace } from './memory';
export { BrowserWorkspace } from './browser';
export {
	hasDirectoryImport,
	importFolder,
	readDirectoryFiles,
	folderNameFrom,
	type DirectoryFileLike
} from './import';
export { zipWorkspace, downloadWorkspace } from './export';
export { FolderWorkspace, WorkspacePermissionError, openFolder, restore } from './folder';
export {
	isPermissionError,
	readHandle,
	writeHandle,
	type DirectoryHandleLike,
	type FileHandleLike,
	type FileLike,
	type PermissionMode,
	type WritableLike
} from './handles';
export {
	setDirectoryPicker,
	hasDirectoryPicker,
	resolvePicker,
	isAbortError,
	type DirectoryPicker
} from './picker';
export {
	rememberHandle,
	recallHandle,
	forgetHandle,
	setHandleStore,
	rememberWorkspace,
	recallWorkspace,
	forgetWorkspace,
	type HandleStore,
	type StoredWorkspace
} from './persistence';
export {
	SKIPPED_NAMES,
	shouldSkip,
	normalizePath,
	dirname,
	basename,
	compareEntries,
	sortTree,
	buildTree
} from './paths';
export { workspace, type WorkspaceStatus } from './store.svelte.js';
