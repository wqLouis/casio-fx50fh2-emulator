/**
 * The editor's palette is the page's palette.
 *
 * Every colour here is a CSS variable — the same `--color-neutral-*` the
 * surrounding interface uses, plus the `--syn-*` syntax colours and `--ui-*`
 * overlays that `layout.css` derives from whichever theme is selected. Nothing
 * is written down as a literal, so the editor follows all of them, including the
 * ports, without this file knowing any of them exist. Adding a theme means
 * adding a palette in `layout.css`; there is nothing to do here.
 *
 * That is why there is no longer a dark theme and a light theme with the colours
 * spelled out twice. The one thing CodeMirror still has to be told is which
 * *kind* of theme it is, because it uses that for its own defaults — the caret,
 * the `&dark`/`&light` class, the built-in widgets it ships. So the theme is
 * built per appearance and swapped through a compartment, while the colours stay
 * in CSS.
 *
 * Tags are mapped to such variables directly: the palette is the only place that
 * needs to change to restyle the syntax.
 */

import { HighlightStyle, syntaxHighlighting } from '@codemirror/language';
import type { Extension } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { tags } from '@lezer/highlight';

import type { EditorTheme } from './types';

/** The editor and the surrounding panels must use the same monospace stack. */
const MONO =
	'ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, "Liberation Mono", monospace';

/** Chrome: gutters, selection, tooltips, diagnostics. Colours are all variables. */
const chrome = {
	'&': {
		color: 'var(--color-neutral-200)',
		backgroundColor: 'transparent',
		height: '100%'
	},
	'.cm-scroller': {
		fontFamily: MONO,
		fontSize: '13px',
		lineHeight: '1.6'
	},
	'.cm-content': { caretColor: 'var(--color-neutral-50)' },
	'.cm-cursor, .cm-dropCursor': { borderLeftColor: 'var(--color-neutral-50)' },
	'&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection':
		{
			backgroundColor: 'var(--ui-selection)'
		},
	'.cm-selectionMatch': { backgroundColor: 'var(--ui-selection-soft)' },
	'.cm-activeLine': { backgroundColor: 'var(--ui-active-line)' },
	// Matching brackets: tinted toward the accent, so it survives greyscale.
	'&.cm-focused .cm-matchingBracket, &.cm-focused .cm-nonmatchingBracket': {
		backgroundColor: 'var(--ui-bracket)',
		outline: '1px solid var(--ui-selection)'
	},
	'.cm-gutters': {
		backgroundColor: 'transparent',
		color: 'var(--color-neutral-500)',
		border: 'none',
		borderRight: '1px solid var(--color-neutral-800)'
	},
	'.cm-activeLineGutter': {
		backgroundColor: 'var(--ui-active-line)',
		color: 'var(--color-neutral-400)'
	},
	'.cm-lineNumbers .cm-gutterElement': { padding: '0 12px 0 8px' },
	'.cm-foldPlaceholder': {
		backgroundColor: 'transparent',
		border: 'none',
		color: 'var(--color-neutral-400)'
	},
	// Search and replace panels.
	'.cm-panels': {
		backgroundColor: 'var(--color-neutral-900)',
		color: 'var(--color-neutral-200)'
	},
	'.cm-panels.cm-panels-top': { borderBottom: '1px solid var(--color-neutral-800)' },
	'.cm-panels.cm-panels-bottom': { borderTop: '1px solid var(--color-neutral-800)' },
	'.cm-textfield': {
		backgroundColor: 'var(--color-neutral-950)',
		color: 'var(--color-neutral-200)',
		border: '1px solid var(--color-neutral-700)'
	},
	'.cm-button': {
		backgroundColor: 'var(--color-neutral-800)',
		backgroundImage: 'none',
		color: 'var(--color-neutral-200)',
		border: '1px solid var(--color-neutral-700)'
	},
	'.cm-searchMatch': {
		backgroundColor: 'var(--ui-search)',
		outline: '1px solid var(--color-neutral-800)'
	},
	'.cm-searchMatch.cm-searchMatch-selected': { backgroundColor: 'var(--ui-search-active)' },
	// Tooltips: hover documentation and the autocomplete list.
	'.cm-tooltip': {
		border: '1px solid var(--color-neutral-800)',
		backgroundColor: 'var(--color-neutral-900)',
		color: 'var(--color-neutral-200)'
	},
	'.cm-tooltip .cm-tooltip-arrow:before': {
		borderTopColor: 'var(--color-neutral-800)',
		borderBottomColor: 'var(--color-neutral-800)'
	},
	'.cm-tooltip .cm-tooltip-arrow:after': {
		borderTopColor: 'var(--color-neutral-900)',
		borderBottomColor: 'var(--color-neutral-900)'
	},
	'.cm-tooltip-autocomplete > ul > li[aria-selected]': {
		backgroundColor: 'var(--color-neutral-800)',
		color: 'var(--color-neutral-50)'
	},
	// CodeMirror ships icons for its documented completion types; LSP's
	// `Operator` is not one of them, so supply the missing glyph rather than
	// mapping operators onto the keyword icon.
	'.cm-completionIcon-operator': { '&:after': { content: "'±'" } },
	// Hover documentation, rendered by `wasm.ts` without innerHTML.
	'.cm-fxc-hover': { maxWidth: '420px', padding: '2px 4px', whiteSpace: 'pre-wrap' },
	'.cm-fxc-hover code': {
		backgroundColor: 'var(--color-neutral-800)',
		borderRadius: '3px',
		padding: '1px 4px',
		fontSize: '90%'
	},
	'.cm-fxc-hover strong': { color: 'var(--color-neutral-50)' },
	'.cm-fxc-hover pre': {
		margin: '6px 0',
		padding: '6px 8px',
		backgroundColor: 'var(--color-neutral-950)',
		borderRadius: '4px',
		overflowX: 'auto'
	},
	'.cm-fxc-hover-heading': {
		color: 'var(--color-neutral-50)',
		fontWeight: '600',
		marginTop: '4px'
	},
	'.cm-fxc-hover-item': { paddingLeft: '10px' },
	// Diagnostics use the interface's accent palette, so an error is the same
	// red in the editor as in the Problems panel.
	'.cm-diagnostic-error': { borderLeft: '3px solid var(--color-red-400)' },
	'.cm-diagnostic-warning': { borderLeft: '3px solid var(--color-amber-400)' },
	'.cm-diagnostic-info': { borderLeft: '3px solid var(--color-sky-400)' },
	'.cm-diagnostic-hint': { borderLeft: '3px solid var(--color-neutral-400)' },
	'.cm-lintRange-error': {
		backgroundImage: 'none',
		textDecoration: 'underline wavy var(--color-red-400)'
	},
	'.cm-lintRange-warning': {
		backgroundImage: 'none',
		textDecoration: 'underline wavy var(--color-amber-400)'
	},
	'.cm-lintRange-info': {
		backgroundImage: 'none',
		textDecoration: 'underline dotted var(--color-sky-400)'
	},
	'.cm-lintRange-hint': {
		backgroundImage: 'none',
		textDecoration: 'underline dotted var(--color-neutral-400)'
	}
};

/** Token tags → the theme's syntax colours. One definition serves every theme. */
const highlight = HighlightStyle.define([
	{ tag: tags.keyword, color: 'var(--syn-keyword)' },
	{ tag: tags.atom, color: 'var(--syn-number)' },
	{ tag: tags.meta, color: 'var(--syn-directive)' }, // directives
	{ tag: tags.namespace, color: 'var(--syn-namespace)' },
	{ tag: tags.standard(tags.name), color: 'var(--syn-builtin)' }, // built-ins
	{ tag: tags.function(tags.variableName), color: 'var(--syn-builtin)' },
	{ tag: tags.constant(tags.name), color: 'var(--syn-number)' },
	{ tag: tags.propertyName, color: 'var(--syn-property)' },
	{ tag: tags.variableName, color: 'var(--color-neutral-200)' },
	{ tag: tags.number, color: 'var(--syn-number)' },
	{ tag: tags.string, color: 'var(--syn-string)' },
	{ tag: tags.comment, color: 'var(--syn-comment)', fontStyle: 'italic' },
	{ tag: tags.operator, color: 'var(--syn-operator)' },
	{ tag: [tags.bracket, tags.punctuation], color: 'var(--syn-punctuation)' }
]);

/** The syntax highlighting as an extension, for callers assembling their own state. */
export const fxcSyntaxHighlighting = syntaxHighlighting(highlight);

/**
 * The editor extensions for a resolved appearance: syntax, then chrome.
 *
 * The only thing `appearance` changes is CodeMirror's own `dark` flag; every
 * colour comes from CSS.
 */
export function editorTheme(appearance: EditorTheme): Extension {
	return [fxcSyntaxHighlighting, EditorView.theme(chrome, { dark: appearance === 'dark' })];
}
