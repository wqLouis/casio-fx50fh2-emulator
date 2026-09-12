/**
 * `.fxc` as a CodeMirror language.
 *
 * `StreamLanguage` is the pragmatic choice: the language is line-oriented
 * enough for a stream tokenizer, and a Lezer grammar would buy incremental
 * parsing the editor does not need — the real parser lives in Rust and is
 * reached through the wasm module. What matters here is that the token table
 * matches `tokenizer.ts`, so a token name has exactly one colour.
 *
 * The same file also defines the PRGM (`.fx`) language from `prgm.ts`, and the
 * `languageSupportForPath` chooser the component uses to pick between them.
 */

import {
	LanguageSupport,
	StreamLanguage,
	bracketMatching,
	type StreamParser
} from '@codemirror/language';
import { tags, type Tag } from '@lezer/highlight';

import { createTokenState, token, type FxcToken, type FxcTokenState } from './tokenizer';
import { createPrgmTokenState, prgmToken, type PrgmToken, type PrgmTokenState } from './prgm';
import { FXC_LANGUAGE_ID, PRGM_LANGUAGE_ID, languageIdForPath } from './types';

/**
 * Token name → highlight tag.
 *
 * The tag, not a literal colour, is the mapping: `theme.ts` supplies the
 * palette, so the language can be restyled without touching the tokenizer.
 */
const TOKEN_TAGS: Record<FxcToken, Tag | readonly Tag[]> = {
	keyword: tags.keyword,
	directive: tags.meta,
	mode: tags.atom,
	namespace: tags.namespace,
	// Built-ins and user functions share the function colour, but built-ins are
	// additionally `standard` so the palette may distinguish them later.
	builtin: [tags.standard(tags.name), tags.function(tags.variableName)],
	constant: tags.constant(tags.name),
	property: tags.propertyName,
	variable: tags.variableName,
	function: tags.function(tags.variableName),
	number: tags.number,
	string: tags.string,
	comment: tags.comment,
	operator: tags.operator,
	bracket: tags.bracket,
	punctuation: tags.punctuation
};

const parser: StreamParser<FxcTokenState> = {
	name: FXC_LANGUAGE_ID,
	startState: createTokenState,
	token,
	tokenTable: TOKEN_TAGS,
	languageData: {
		// Read by the comment commands and by `EditorState.languageDataAt`, so
		// Ctrl-/ and the block-comment command know the syntax without the
		// component repeating it.
		commentTokens: { line: '//', block: { open: '/*', close: '*/' } },
		closeBrackets: { brackets: ['(', '[', '{', '"'] }
	}
};

/** The `.fxc` language, for `EditorState` and for language-data lookups. */
export const fxcLanguage = StreamLanguage.define(parser);

/**
 * The language plus the bracket-matching extension that pairs with it. The
 * component can spread this in directly; callers that want the bare language
 * can use `fxcLanguage`.
 */
export const fxcLanguageSupport = new LanguageSupport(fxcLanguage, [bracketMatching()]);

/**
 * Token name → highlight tag for PRGM.
 *
 * The palette is the same `theme.ts` one the `.fxc` language uses, so the two
 * languages look like siblings; only the token vocabulary differs.
 */
const PRGM_TOKEN_TAGS: Record<PrgmToken, Tag | readonly Tag[]> = {
	keyword: tags.keyword,
	directive: tags.meta,
	mode: tags.atom,
	// Program keys and built-ins share the function colour, with `standard` for
	// the palette to distinguish later, exactly as the `.fxc` map does.
	function: [tags.standard(tags.name), tags.function(tags.variableName)],
	namespace: tags.namespace,
	constant: tags.constant(tags.name),
	variable: tags.variableName,
	statistic: tags.constant(tags.name),
	number: tags.number,
	comment: tags.comment,
	operator: tags.operator,
	postfix: tags.operator,
	bracket: tags.bracket,
	punctuation: tags.punctuation
};

const prgmParser: StreamParser<PrgmTokenState> = {
	name: PRGM_LANGUAGE_ID,
	startState: createPrgmTokenState,
	token: prgmToken,
	tokenTable: PRGM_TOKEN_TAGS,
	languageData: {
		// PRGM has no block comments; only `//` to end of line.
		commentTokens: { line: '//' },
		closeBrackets: { brackets: ['('] }
	}
};

/** The PRGM language, for `EditorState` and for language-data lookups. */
export const prgmLanguage = StreamLanguage.define(prgmParser);

/** PRGM plus bracket matching, the `.fx` counterpart of `fxcLanguageSupport`. */
export const prgmLanguageSupport = new LanguageSupport(prgmLanguage, [bracketMatching()]);

/**
 * The language support for a document path, mirroring `languageIdForPath`.
 *
 * `CodeEditor` reconfigures its language compartment through this when the
 * active path changes, so switching from `main.fxc` to `prog.fx` swaps the
 * highlighter as well as the completions and diagnostics.
 */
export function languageSupportForPath(path: string): LanguageSupport {
	return languageIdForPath(path) === PRGM_LANGUAGE_ID ? prgmLanguageSupport : fxcLanguageSupport;
}

export { FXC_LANGUAGE_ID, PRGM_LANGUAGE_ID, languageIdForPath };
export type { FxcToken, FxcTokenState, PrgmToken, PrgmTokenState };
