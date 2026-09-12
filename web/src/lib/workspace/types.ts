/**
 * The workspace contract.
 *
 * A workspace is the folder the user is editing. There are two backends behind
 * this interface:
 *
 * * **folder** — a real directory on disk, opened through the File System Access
 *   API (`showDirectoryPicker`). Reads and writes go to the user's actual files.
 * * **memory** — a map held in the page. It is what a browser without the File
 *   System Access API falls back to, what "new scratch file" uses, and what the
 *   tests drive, because a native directory picker cannot be automated.
 *
 * Everything above this layer works the same either way, so the editor does not
 * have to care which one is in use beyond what `capabilities` reports.
 */

/** A path relative to the workspace root, always POSIX-style: `lib/pack.fxc`. */
export type WorkspacePath = string;

/**
 * Which backend is in use.
 *
 * `folder` is a real directory on disk. `browser` is a workspace kept in the
 * browser's own storage — what a browser without the File System Access API
 * gets, and what the simple editor route uses. `memory` is neither: it is
 * discarded when the page goes away, and exists so tests have something with no
 * storage behind it at all.
 */
export type WorkspaceKind = 'folder' | 'browser' | 'memory';

/** What the current backend can do, for the UI to grey out what it cannot. */
export interface WorkspaceCapabilities {
	/** Whether a real folder can be opened at all (the API is present). */
	canOpenFolder: boolean;
	/** Whether edits are written back to a real folder. */
	canWrite: boolean;
	/**
	 * Whether a folder can be imported read-only.
	 *
	 * Firefox has no File System Access API, so this is the only way to get a
	 * real folder's files in: `<input type="file" webkitdirectory>`. It yields
	 * the tree but can never write back, which is why it is a separate
	 * capability from `canOpenFolder` — and why `canWrite` stays false when it
	 * is the one in use. Edits then live in the browser and are exported.
	 */
	canImportFolder: boolean;
	/**
	 * Whether a reload will need the user to re-grant access. Browsers drop
	 * permission on reload even when the handle itself is remembered.
	 */
	needsPermission: boolean;
}

/** One entry in the file tree. */
export interface WorkspaceEntry {
	path: WorkspacePath;
	name: string;
	kind: 'file' | 'directory';
	/** Present on directories only; empty for an empty directory. */
	children?: WorkspaceEntry[];
}

/**
 * A read/write view of the user's folder.
 *
 * Implementations are responsible for keeping their own tree in step after a
 * write, create, rename or remove, so that `entries()` is correct immediately
 * afterwards rather than only after a re-open.
 */
export interface Workspace {
	readonly kind: WorkspaceKind;
	/** The folder's name, for the title bar. */
	readonly name: string;
	readonly capabilities: WorkspaceCapabilities;

	/** The tree, sorted: directories first, then files, each alphabetically. */
	entries(): WorkspaceEntry[];

	read(path: WorkspacePath): Promise<string>;
	write(path: WorkspacePath, text: string): Promise<void>;
	/** Create an empty file (or one with `text`). Parent directories are created. */
	create(path: WorkspacePath, text?: string): Promise<void>;
	remove(path: WorkspacePath): Promise<void>;
	/** Move or rename. Fails if `to` already exists. */
	rename(from: WorkspacePath, to: WorkspacePath): Promise<void>;

	/**
	 * Every text file's contents, keyed by workspace-relative path.
	 *
	 * This is exactly the `files` map the wasm API takes, which is how
	 * `#include` resolves against the real folder.
	 */
	snapshot(): Promise<Record<WorkspacePath, string>>;
}
