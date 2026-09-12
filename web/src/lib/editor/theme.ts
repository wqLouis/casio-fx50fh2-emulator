/**
 * The editor's dark theme.
 *
 * It uses the same neutral palette as the surrounding Tailwind 4 app
 * (`neutral-950 … neutral-100`), so the editor reads as part of the page
 * rather than a separately themed widget. The editor surface is transparent;
 * the component's wrapper decides whether it sits on `neutral-900` or
 * `neutral-950`.
 *
 * Colours are defined as tags here, not token names, because `language.ts`
 * maps tokens to `@lezer/highlight` tags — the palette is the only place that
 * needs to change to restyle the syntax.
 */

import { HighlightStyle, syntaxHighlighting } from '@codemirror/language';
import type { Extension } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { tags } from '@lezer/highlight';

import type { EditorTheme } from './types';

/** Neutral-200 text on a transparent surface, with neutral-800 borders. */
export const fxcTheme = EditorView.theme(
	{
		'&': {
			color: '#e5e5e5', // neutral-200
			backgroundColor: 'transparent',
			height: '100%'
		},
		'.cm-scroller': {
			fontFamily:
				'ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, "Liberation Mono", monospace',
			fontSize: '13px',
			lineHeight: '1.6'
		},
		'.cm-content': {
			caretColor: '#fafafa' // neutral-50
		},
		'.cm-cursor, .cm-dropCursor': { borderLeftColor: '#fafafa' },
		'&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection':
			{
				backgroundColor: '#404040' // neutral-700
			},
		'.cm-selectionMatch': { backgroundColor: 'rgba(82, 82, 82, 0.5)' }, // neutral-600/50
		'.cm-activeLine': { backgroundColor: 'rgba(255, 255, 255, 0.03)' },
		// Matching brackets: a quiet blue that survives greyscale.
		'&.cm-focused .cm-matchingBracket, &.cm-focused .cm-nonmatchingBracket': {
			backgroundColor: 'rgba(96, 165, 250, 0.22)',
			outline: '1px solid rgba(96, 165, 250, 0.45)'
		},
		'.cm-gutters': {
			backgroundColor: 'transparent',
			color: '#737373', // neutral-500
			border: 'none',
			borderRight: '1px solid #262626' // neutral-800
		},
		'.cm-activeLineGutter': {
			backgroundColor: 'rgba(255, 255, 255, 0.03)',
			color: '#a3a3a3' // neutral-400
		},
		'.cm-lineNumbers .cm-gutterElement': { padding: '0 12px 0 8px' },
		'.cm-foldPlaceholder': {
			backgroundColor: 'transparent',
			border: 'none',
			color: '#a3a3a3'
		},
		// Search and replace panels.
		'.cm-panels': { backgroundColor: '#171717', color: '#e5e5e5' }, // neutral-900
		'.cm-panels.cm-panels-top': { borderBottom: '1px solid #262626' },
		'.cm-panels.cm-panels-bottom': { borderTop: '1px solid #262626' },
		'.cm-textfield': {
			backgroundColor: '#0a0a0a', // neutral-950
			color: '#e5e5e5',
			border: '1px solid #404040'
		},
		'.cm-button': {
			backgroundColor: '#262626',
			backgroundImage: 'none',
			color: '#e5e5e5',
			border: '1px solid #404040'
		},
		'.cm-searchMatch': { backgroundColor: '#854d0e', outline: '1px solid #a16207' },
		'.cm-searchMatch.cm-searchMatch-selected': { backgroundColor: '#a16207' },
		// Tooltips: hover documentation and the autocomplete list.
		'.cm-tooltip': {
			border: '1px solid #262626',
			backgroundColor: '#171717',
			color: '#e5e5e5'
		},
		'.cm-tooltip .cm-tooltip-arrow:before': {
			borderTopColor: '#262626',
			borderBottomColor: '#262626'
		},
		'.cm-tooltip .cm-tooltip-arrow:after': {
			borderTopColor: '#171717',
			borderBottomColor: '#171717'
		},
		'.cm-tooltip-autocomplete > ul > li[aria-selected]': {
			backgroundColor: '#262626',
			color: '#fafafa'
		},
		// CodeMirror ships icons for its documented completion types; LSP's
		// `Operator` is not one of them, so supply the missing glyph rather than
		// mapping operators onto the keyword icon.
		'.cm-completionIcon-operator': { '&:after': { content: "'±'" } },
		// Hover documentation, rendered by `wasm.ts` without innerHTML.
		'.cm-fxc-hover': { maxWidth: '420px', padding: '2px 4px', whiteSpace: 'pre-wrap' },
		'.cm-fxc-hover code': {
			backgroundColor: '#262626',
			borderRadius: '3px',
			padding: '1px 4px',
			fontSize: '90%'
		},
		'.cm-fxc-hover strong': { color: '#fafafa' },
		'.cm-fxc-hover pre': {
			margin: '6px 0',
			padding: '6px 8px',
			backgroundColor: '#0a0a0a',
			borderRadius: '4px',
			overflowX: 'auto'
		},
		'.cm-fxc-hover-heading': { color: '#fafafa', fontWeight: '600', marginTop: '4px' },
		'.cm-fxc-hover-item': { paddingLeft: '10px' },
		// Diagnostics.
		'.cm-diagnostic-error': { borderLeft: '3px solid #f87171' },
		'.cm-diagnostic-warning': { borderLeft: '3px solid #fbbf24' },
		'.cm-diagnostic-info': { borderLeft: '3px solid #60a5fa' },
		'.cm-diagnostic-hint': { borderLeft: '3px solid #a3a3a3' },
		'.cm-lintRange-error': {
			backgroundImage: 'none',
			textDecoration: 'underline wavy #f87171'
		},
		'.cm-lintRange-warning': {
			backgroundImage: 'none',
			textDecoration: 'underline wavy #fbbf24'
		},
		'.cm-lintRange-info': { backgroundImage: 'none', textDecoration: 'underline dotted #60a5fa' },
		'.cm-lintRange-hint': { backgroundImage: 'none', textDecoration: 'underline dotted #a3a3a3' }
	},
	{ dark: true }
);

/** Token tags → a restrained dark palette. */
export const fxcHighlightStyle = HighlightStyle.define([
	{ tag: tags.keyword, color: '#c084fc' }, // violet-400
	{ tag: tags.atom, color: '#fbbf24' }, // amber-400
	{ tag: tags.meta, color: '#f472b6' }, // pink-400 — directives
	{ tag: tags.namespace, color: '#22d3ee' }, // cyan-400
	{ tag: tags.standard(tags.name), color: '#60a5fa' }, // blue-400 — built-ins
	{ tag: tags.function(tags.variableName), color: '#60a5fa' },
	{ tag: tags.constant(tags.name), color: '#fbbf24' },
	{ tag: tags.propertyName, color: '#a5b4fc' }, // indigo-300
	{ tag: tags.variableName, color: '#e5e5e5' },
	{ tag: tags.number, color: '#fb923c' }, // orange-400
	{ tag: tags.string, color: '#4ade80' }, // green-400
	{ tag: tags.comment, color: '#737373', fontStyle: 'italic' },
	{ tag: tags.operator, color: '#f87171' }, // red-400
	{ tag: [tags.bracket, tags.punctuation], color: '#a3a3a3' } // neutral-400
]);

/** The highlight style as an extension, for convenience. */
export const fxcSyntaxHighlighting = syntaxHighlighting(fxcHighlightStyle);

/**
 * The editor's light theme.
 *
 * The same structure as the dark one with the palette reflected: the neutrals
 * come from the other end of the ramp, and the syntax colours move two or three
 * steps darker so they still carry on a near-white surface. It is a separate
 * definition rather than the dark one with variables because CodeMirror's
 * `{ dark: false }` flag also drives its own built-in defaults, which were
 * chosen for the light ramp.
 */
export const fxcLightTheme = EditorView.theme(
	{
		'&': {
			color: '#262626', // neutral-800
			backgroundColor: 'transparent',
			height: '100%'
		},
		'.cm-scroller': {
			fontFamily:
				'ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, "Liberation Mono", monospace',
			fontSize: '13px',
			lineHeight: '1.6'
		},
		'.cm-content': { caretColor: '#171717' },
		'.cm-cursor, .cm-dropCursor': { borderLeftColor: '#171717' },
		'&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection':
			{
				backgroundColor: 'rgba(37, 99, 235, 0.18)'
			},
		'.cm-selectionMatch': { backgroundColor: 'rgba(37, 99, 235, 0.12)' },
		'.cm-activeLine': { backgroundColor: 'rgba(0, 0, 0, 0.035)' },
		'&.cm-focused .cm-matchingBracket, &.cm-focused .cm-nonmatchingBracket': {
			backgroundColor: 'rgba(37, 99, 235, 0.14)',
			outline: '1px solid rgba(37, 99, 235, 0.4)'
		},
		'.cm-gutters': {
			backgroundColor: 'transparent',
			color: '#737373', // neutral-500
			border: 'none',
			borderRight: '1px solid #e5e5e5' // neutral-200
		},
		'.cm-activeLineGutter': {
			backgroundColor: 'rgba(0, 0, 0, 0.035)',
			color: '#525252' // neutral-600
		},
		'.cm-lineNumbers .cm-gutterElement': { padding: '0 12px 0 8px' },
		'.cm-foldPlaceholder': {
			backgroundColor: 'transparent',
			border: 'none',
			color: '#525252'
		},
		// Search and replace panels.
		'.cm-panels': { backgroundColor: '#f5f5f5', color: '#262626' }, // neutral-100
		'.cm-panels.cm-panels-top': { borderBottom: '1px solid #e5e5e5' },
		'.cm-panels.cm-panels-bottom': { borderTop: '1px solid #e5e5e5' },
		'.cm-textfield': {
			backgroundColor: '#ffffff',
			color: '#262626',
			border: '1px solid #d4d4d4'
		},
		'.cm-button': {
			backgroundColor: '#f5f5f5',
			backgroundImage: 'none',
			color: '#262626',
			border: '1px solid #d4d4d4'
		},
		'.cm-searchMatch': { backgroundColor: '#fde68a', outline: '1px solid #f59e0b' },
		'.cm-searchMatch.cm-searchMatch-selected': { backgroundColor: '#fcd34d' },
		// Tooltips: hover documentation and the autocomplete list.
		'.cm-tooltip': {
			border: '1px solid #e5e5e5',
			backgroundColor: '#ffffff',
			color: '#262626'
		},
		'.cm-tooltip .cm-tooltip-arrow:before': {
			borderTopColor: '#e5e5e5',
			borderBottomColor: '#e5e5e5'
		},
		'.cm-tooltip .cm-tooltip-arrow:after': {
			borderTopColor: '#ffffff',
			borderBottomColor: '#ffffff'
		},
		'.cm-tooltip-autocomplete > ul > li[aria-selected]': {
			backgroundColor: '#f5f5f5',
			color: '#171717'
		},
		'.cm-completionIcon-operator': { '&:after': { content: "'±'" } },
		'.cm-fxc-hover': { maxWidth: '420px', padding: '2px 4px', whiteSpace: 'pre-wrap' },
		'.cm-fxc-hover code': {
			backgroundColor: '#f5f5f5',
			borderRadius: '3px',
			padding: '1px 4px',
			fontSize: '90%'
		},
		'.cm-fxc-hover strong': { color: '#171717' },
		'.cm-fxc-hover pre': {
			margin: '6px 0',
			padding: '6px 8px',
			backgroundColor: '#f5f5f5',
			borderRadius: '4px',
			overflowX: 'auto'
		},
		'.cm-fxc-hover-heading': { color: '#171717', fontWeight: '600', marginTop: '4px' },
		'.cm-fxc-hover-item': { paddingLeft: '10px' },
		// Diagnostics.
		'.cm-diagnostic-error': { borderLeft: '3px solid #dc2626' },
		'.cm-diagnostic-warning': { borderLeft: '3px solid #d97706' },
		'.cm-diagnostic-info': { borderLeft: '3px solid #2563eb' },
		'.cm-diagnostic-hint': { borderLeft: '3px solid #737373' },
		'.cm-lintRange-error': {
			backgroundImage: 'none',
			textDecoration: 'underline wavy #dc2626'
		},
		'.cm-lintRange-warning': {
			backgroundImage: 'none',
			textDecoration: 'underline wavy #d97706'
		},
		'.cm-lintRange-info': { backgroundImage: 'none', textDecoration: 'underline dotted #2563eb' },
		'.cm-lintRange-hint': { backgroundImage: 'none', textDecoration: 'underline dotted #737373' }
	},
	{ dark: false }
);

/** Token tags → a light palette, the reflection of the dark one. */
export const fxcLightHighlightStyle = HighlightStyle.define([
	{ tag: tags.keyword, color: '#7c3aed' }, // violet-600
	{ tag: tags.atom, color: '#b45309' }, // amber-700
	{ tag: tags.meta, color: '#be185d' }, // pink-700 — directives
	{ tag: tags.namespace, color: '#0e7490' }, // cyan-700
	{ tag: tags.standard(tags.name), color: '#1d4ed8' }, // blue-700 — built-ins
	{ tag: tags.function(tags.variableName), color: '#1d4ed8' },
	{ tag: tags.constant(tags.name), color: '#b45309' },
	{ tag: tags.propertyName, color: '#4338ca' }, // indigo-700
	{ tag: tags.variableName, color: '#262626' },
	{ tag: tags.number, color: '#c2410c' }, // orange-700
	{ tag: tags.string, color: '#15803d' }, // green-700
	{ tag: [tags.comment], color: '#737373', fontStyle: 'italic' },
	{ tag: tags.operator, color: '#b91c1c' }, // red-700
	{ tag: [tags.bracket, tags.punctuation], color: '#525252' } // neutral-600
]);

/**
 * The editor extensions for a resolved theme: chrome and syntax together.
 *
 * They are returned as one bundle because they have to move as a unit — a light
 * surface carrying the dark syntax palette is unreadable — so a single
 * compartment can swap both at once.
 */
export function editorTheme(theme: EditorTheme): Extension {
	return theme === 'dark'
		? [fxcSyntaxHighlighting, fxcTheme]
		: [syntaxHighlighting(fxcLightHighlightStyle), fxcLightTheme];
}
