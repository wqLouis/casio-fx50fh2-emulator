/**
 * The bridge from the WebAssembly module's LSP-shaped operations to
 * CodeMirror.
 *
 * The module returns *standard* LSP values (`lsp-types`, serialised straight
 * through). CodeMirror has its own, much smaller shapes, so the work here is a
 * translation in one direction and nothing else:
 *
 *   * `DiagnosticSeverity` integer → CodeMirror's severity string,
 *   * `CompletionItemKind` integer → the completion `type` string,
 *   * `InsertTextFormat.Snippet` → a CodeMirror snippet completion,
 *   * LSP `Position` (0-based line/character) → a document offset,
 *   * `Hover` `MarkupContent` → tooltip text.
 *
 * Every translation is a pure function so it can be tested without a DOM; the
 * three `create…Source` helpers are the only things that touch a live view.
 * Each returns a no-op when `fx` is null, which is the state before the module
 * has finished loading and is normal, not an error.
 */

import {
	snippetCompletion,
	type Completion,
	type CompletionSource
} from '@codemirror/autocomplete';
import type { Diagnostic as CodeMirrorDiagnostic, LintSource } from '@codemirror/lint';
import type { Text } from '@codemirror/state';
import type { HoverTooltipSource } from '@codemirror/view';
import {
	CompletionItemKind,
	DiagnosticSeverity,
	InsertTextFormat,
	type CompletionItem,
	type Diagnostic,
	type Hover,
	type MarkupContent,
	type Position
} from 'vscode-languageserver-types';

import type { Fx50 } from '$lib/fx50';

import { languageIdForPath } from './types';

/**
 * The entry path the wasm module itself defaults to when a request omits one.
 * An absent `path` therefore has to be read as this, or the language we send
 * would disagree with the entry the module actually uses to resolve includes.
 */
const DEFAULT_ENTRY = 'main.fxc';

/** What the sources need to know, resolved afresh on every request. */
export interface WasmEditorContext {
	/** Absent until the module has loaded. */
	fx?: Fx50 | null;
	/**
	 * The workspace-relative path of the document. It is the `#include` entry and,
	 * through `languageIdForPath`, the language the request is made in.
	 */
	path?: string;
	/** The rest of the workspace, keyed by path. */
	files?: Record<string, string>;
}

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/**
 * LSP severity is an integer; CodeMirror's is a string.
 *
 * `Error` is `1` in the protocol, not `0`, and the module only ever produces
 * errors today — but Warning/Information/Hint are mapped anyway so a future
 * server-side severity needs no change here.
 */
export function severityFor(
	severity: DiagnosticSeverity | undefined
): CodeMirrorDiagnostic['severity'] {
	switch (severity) {
		case DiagnosticSeverity.Warning:
			return 'warning';
		case DiagnosticSeverity.Information:
			return 'info';
		case DiagnosticSeverity.Hint:
			return 'hint';
		case DiagnosticSeverity.Error:
		default:
			return 'error';
	}
}

/**
 * LSP completion kinds are integers; CodeMirror's `type` picks the icon.
 *
 * The documented CodeMirror types are `class`, `constant`, `enum`, `function`,
 * `interface`, `keyword`, `method`, `namespace`, `property`, `text`, `type`
 * and `variable`. `operator` is not among them, so the theme adds its glyph.
 */
export function completionTypeFor(kind: CompletionItemKind | undefined): string | undefined {
	switch (kind) {
		case CompletionItemKind.Method:
			return 'method';
		case CompletionItemKind.Function:
			return 'function';
		case CompletionItemKind.Constructor:
			return 'class';
		case CompletionItemKind.Field:
		case CompletionItemKind.Property:
			return 'property';
		case CompletionItemKind.Variable:
			return 'variable';
		case CompletionItemKind.Class:
			return 'class';
		case CompletionItemKind.Interface:
			return 'interface';
		case CompletionItemKind.Module:
			return 'namespace';
		case CompletionItemKind.Enum:
		case CompletionItemKind.EnumMember:
			return 'enum';
		case CompletionItemKind.Keyword:
			return 'keyword';
		case CompletionItemKind.Constant:
			return 'constant';
		case CompletionItemKind.Struct:
		case CompletionItemKind.TypeParameter:
			return 'type';
		case CompletionItemKind.Operator:
			return 'operator';
		case undefined:
			return undefined;
		default:
			return 'text';
	}
}

// ---------------------------------------------------------------------------
// Completions
// ---------------------------------------------------------------------------

/** Flatten the several shapes LSP allows for `documentation`. */
function documentationText(documentation: CompletionItem['documentation']): string | undefined {
	if (documentation === undefined) return undefined;
	if (typeof documentation === 'string') return documentation;
	if (Array.isArray(documentation)) {
		const text = documentation
			.map((part) => (typeof part === 'string' ? part : part.value))
			.join('\n\n');
		return text || undefined;
	}
	return documentation.value || undefined;
}

/** One LSP completion as a CodeMirror completion. */
export function toCodeMirrorCompletion(item: CompletionItem): Completion {
	const completion: Completion = {
		label: item.label,
		detail: item.detail,
		sortText: item.sortText,
		type: completionTypeFor(item.kind)
	};

	const info = documentationText(item.documentation);
	if (info) completion.info = info;

	const insert = item.insertText;
	if (insert) {
		if (item.insertTextFormat === InsertTextFormat.Snippet) {
			// `snippetCompletion` turns `$0`/`${1:…}` into a tabbed template.
			return snippetCompletion(insert, completion);
		}
		// The default apply text is the label, so only override it when the
		// server wants something different.
		if (insert !== item.label) completion.apply = insert;
	}

	return completion;
}

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

/**
 * An LSP position → an absolute document offset.
 *
 * LSP counts UTF-16 code units from the start of the line; so does JavaScript,
 * so for `.fxc` (which is BMP-only) the character is added directly. The line
 * and character are clamped, because a diagnostic may briefly outlive an edit
 * that shortened the document.
 */
export function toOffset(doc: Text, position: Position): number {
	const lineNumber = Math.min(Math.max(position.line + 1, 1), doc.lines);
	const line = doc.line(lineNumber);
	return Math.min(line.from + Math.max(position.character, 0), line.to);
}

/** LSP 3.18 widened `Diagnostic.message` to allow `MarkupContent`; flatten it. */
function messageText(message: string | MarkupContent): string {
	return typeof message === 'string' ? message : message.value;
}

/** One LSP diagnostic as a CodeMirror diagnostic. */
export function toCodeMirrorDiagnostic(diagnostic: Diagnostic, doc: Text): CodeMirrorDiagnostic {
	const from = toOffset(doc, diagnostic.range.start);
	const to = Math.max(from, toOffset(doc, diagnostic.range.end));
	const result: CodeMirrorDiagnostic = {
		from,
		to,
		severity: severityFor(diagnostic.severity),
		message: messageText(diagnostic.message)
	};
	if (diagnostic.source) result.source = diagnostic.source;
	return result;
}

// ---------------------------------------------------------------------------
// Hover
// ---------------------------------------------------------------------------

/** Flatten an LSP `Hover`'s contents to Markdown text, or `null` when empty. */
export function hoverText(hover: Hover | null): string | null {
	if (!hover) return null;
	const { contents } = hover;
	if (typeof contents === 'string') return contents || null;
	if (Array.isArray(contents)) {
		const text = contents
			.map((part) => (typeof part === 'string' ? part : part.value))
			.join('\n\n');
		return text || null;
	}
	return contents.value || null;
}

/**
 * Render a small, deliberate subset of Markdown for the hover tooltip.
 *
 * It never uses `innerHTML`: the strings come from our own module, but an
 * editor has no business turning documentation into markup. Backtick spans,
 * `**bold**`, headings, list items and fenced code are enough for the
 * descriptions the language server produces.
 */
function renderInline(text: string, parent: Node): void {
	const pattern = /(`[^`]*`|\*\*[^*]+\*\*)/g;
	let last = 0;
	for (const match of text.matchAll(pattern)) {
		const index = match.index ?? 0;
		if (index > last) parent.appendChild(document.createTextNode(text.slice(last, index)));
		const token = match[0];
		if (token.startsWith('`')) {
			const code = document.createElement('code');
			code.textContent = token.slice(1, -1);
			parent.appendChild(code);
		} else {
			const strong = document.createElement('strong');
			strong.textContent = token.slice(2, -2);
			parent.appendChild(strong);
		}
		last = index + token.length;
	}
	if (last < text.length) parent.appendChild(document.createTextNode(text.slice(last)));
}

function hoverDom(markdown: string): HTMLElement {
	const dom = document.createElement('div');
	dom.className = 'cm-fxc-hover';

	let fence: HTMLPreElement | null = null;
	for (const line of markdown.split('\n')) {
		if (line.startsWith('```')) {
			fence = fence ? null : document.createElement('pre');
			if (fence) dom.appendChild(fence);
			continue;
		}
		if (fence) {
			fence.appendChild(document.createTextNode(`${line}\n`));
			continue;
		}
		const row = document.createElement('div');
		if (/^#{1,6}\s/.test(line)) {
			row.className = 'cm-fxc-hover-heading';
			row.textContent = line.replace(/^#{1,6}\s+/, '');
		} else if (/^\s*[-*]\s+/.test(line)) {
			row.className = 'cm-fxc-hover-item';
			renderInline(line.replace(/^\s*[-*]\s+/, '• '), row);
		} else {
			renderInline(line, row);
		}
		dom.appendChild(row);
	}
	return dom;
}

// ---------------------------------------------------------------------------
// The sources
// ---------------------------------------------------------------------------

/**
 * Diagnostics for the linter.
 *
 * The linter debounces this (the component asks for a delay), so a full
 * transpile does not run on every keystroke. A failed or absent module yields
 * no markers rather than an exception, which would blank the editor.
 */
export function createLintSource(get: () => WasmEditorContext): LintSource {
	return async (view) => {
		const { fx, path, files } = get();
		if (!fx) return [];
		const response = fx.diagnostics({
			source: view.state.doc.toString(),
			entry: path,
			files,
			language: languageIdForPath(path ?? DEFAULT_ENTRY)
		});
		if (!response.ok) return [];
		return response.diagnostics.map((diagnostic) =>
			toCodeMirrorDiagnostic(diagnostic, view.state.doc)
		);
	};
}

/**
 * The completion list.
 *
 * The module's list is context-free and language-wide, so the only context
 * that matters is the word being completed.
 */
export function createCompletionSource(get: () => WasmEditorContext): CompletionSource {
	return (context) => {
		const { fx, path } = get();
		if (!fx) return null;
		const word = context.matchBefore(/[#A-Za-z_][\w.]*/);
		if (!word && !context.explicit) return null;
		const response = fx.completions(languageIdForPath(path ?? DEFAULT_ENTRY));
		if (!response.ok) return null;
		return {
			from: word ? word.from : context.pos,
			options: response.items.map(toCodeMirrorCompletion),
			validFor: /^[#A-Za-z_][\w.]*$/
		};
	};
}

/** Documentation at the pointer. */
export function createHoverSource(get: () => WasmEditorContext): HoverTooltipSource {
	return async (view, pos) => {
		const { fx, path, files } = get();
		if (!fx) return null;
		const line = view.state.doc.lineAt(pos);
		const response = fx.hover({
			source: view.state.doc.toString(),
			entry: path,
			files,
			language: languageIdForPath(path ?? DEFAULT_ENTRY),
			position: { line: line.number - 1, character: pos - line.from }
		});
		if (!response.ok || !response.hover) return null;
		const text = hoverText(response.hover);
		if (!text) return null;

		const range = response.hover.range;
		const from = range ? toOffset(view.state.doc, range.start) : pos;
		const to = range ? Math.max(from, toOffset(view.state.doc, range.end)) : pos;
		return { pos: from, end: to, create: () => ({ dom: hoverDom(text) }) };
	};
}
