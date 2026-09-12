/**
 * `$lib/editor` — the public surface.
 *
 * The routes layer only needs `CodeEditor` and, for labels and the wasm calls
 * it makes itself, the language ids. The languages, theme and bridge are
 * exported as well so a route can build its own `EditorState` if it needs to,
 * without reaching into file paths. The two tokenizers stay internal, exactly
 * as `tokenizer.ts` did before `prgm.ts` joined it.
 */

export { default as CodeEditor } from './CodeEditor.svelte';
export {
	FXC_LANGUAGE_ID,
	PRGM_LANGUAGE_ID,
	languageIdForPath,
	type CodeEditorProps,
	type EditorLanguage,
	type EditorTheme
} from './types';
export {
	fxcLanguage,
	fxcLanguageSupport,
	languageSupportForPath,
	prgmLanguage,
	prgmLanguageSupport
} from './language';
export {
	editorTheme,
	fxcHighlightStyle,
	fxcLightHighlightStyle,
	fxcLightTheme,
	fxcSyntaxHighlighting,
	fxcTheme
} from './theme';
export {
	completionTypeFor,
	createCompletionSource,
	createHoverSource,
	createLintSource,
	hoverText,
	severityFor,
	toCodeMirrorCompletion,
	toCodeMirrorDiagnostic,
	toOffset,
	type WasmEditorContext
} from './wasm';
