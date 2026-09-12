/**
 * The editor contract.
 *
 * The component is deliberately ignorant of the workspace: it is given text and
 * a path, and it reports changes. Where that text came from — a real folder, an
 * in-memory fallback, a fresh document — is not its business. The wasm module is
 * passed in rather than imported so the component can be rendered before the
 * module has loaded, and so it can be exercised against a stub.
 */

import type { EditorView } from '@codemirror/view';
import type { Diagnostic } from 'vscode-languageserver-types';

import type { Fx50 } from '$lib/fx50';

export interface CodeEditorProps {
	/** The document text. Replacing it resets the document. */
	value: string;

	/** The module used to answer completions and hover. May be absent early on. */
	fx?: Fx50 | null;

	/** The active file's workspace-relative path, e.g. `main.fxc`. */
	path?: string;

	/**
	 * The rest of the workspace, for `#include`.
	 *
	 * Completions and hover resolve includes through this, so a program that
	 * includes a library gets documentation for the library's functions too.
	 */
	files?: Record<string, string>;

	/** Markers to display. Positions are LSP-style: zero-based. */
	diagnostics?: Diagnostic[];

	/** Called on every edit. */
	onchange?: (value: string) => void;

	/** Called once the view exists, for callers that need to drive it. */
	onready?: (view: EditorView) => void;

	/**
	 * Which palette the editor draws with.
	 *
	 * A prop rather than something the editor reads off the document, so the
	 * component stays usable outside the workbench and testable without a
	 * document class on the root element.
	 */
	theme?: EditorTheme;
}

/** The language id used for `.fxc` documents, matching the editor extensions. */
export const FXC_LANGUAGE_ID = 'fxc';

/** The language id used for `.fx` (PRGM) documents, matching the extensions. */
export const PRGM_LANGUAGE_ID = 'fx';

/** Which of the two editor languages a document is in. */
export type EditorLanguage = typeof PRGM_LANGUAGE_ID | typeof FXC_LANGUAGE_ID;

/** The two appearances the editor can draw. `system` is resolved before it gets here. */
export type EditorTheme = 'light' | 'dark';

/**
 * The language of a document path.
 *
 * This mirrors `Language::from_path` in `crates/fx-lsp/src/logic.rs` exactly,
 * because the wasm module falls back to that same function when a request
 * omits a language: a case-insensitive `.fxc` suffix is the C-like language,
 * and every other path — `.fx`, no extension, an unknown extension — is PRGM.
 *
 * It lives here, apart from the CodeMirror language and the wasm bridge, so it
 * can be unit-tested without a DOM, and so both of those consumers derive the
 * language the same way.
 */
export function languageIdForPath(path: string): EditorLanguage {
	return path.toLowerCase().endsWith(`.${FXC_LANGUAGE_ID}`) ? FXC_LANGUAGE_ID : PRGM_LANGUAGE_ID;
}
