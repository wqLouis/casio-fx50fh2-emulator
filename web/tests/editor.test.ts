/**
 * Unit tests for the editor layer.
 *
 * CodeMirror needs a DOM to *run*, and `bun test` has none — so nothing here
 * constructs an `EditorView`. The pieces that are pure are extracted precisely
 * so they can be tested: `tokenizer.ts` (feed it strings, assert token kinds)
 * and the mapping helpers in `wasm.ts` (feed it LSP shapes, assert CodeMirror
 * shapes). The three `create…Source` functions are driven against stub modules
 * and fake views whose only property is `state.doc`.
 *
 * What is deliberately not covered here — hover DOM rendering, the linter's
 * timing, autocomplete popup behaviour, `value`/`onchange` round trips — only
 * exists in a browser and is called out in the report.
 */

import { describe, expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';
import { CompletionContext } from '@codemirror/autocomplete';
import { EditorState, Text } from '@codemirror/state';
import type { EditorView } from '@codemirror/view';
import {
	CompletionItemKind,
	DiagnosticSeverity,
	InsertTextFormat,
	type CompletionItem,
	type Diagnostic,
	type Hover
} from 'vscode-languageserver-types';

import { examples } from '../src/lib/examples.generated';
import { tokenizePrgm, tokenizePrgmLine, type PrgmToken } from '../src/lib/editor/prgm';
import {
	fxcLanguageSupport,
	languageSupportForPath,
	prgmLanguageSupport
} from '../src/lib/editor/language';
import { FXC_LANGUAGE_ID, PRGM_LANGUAGE_ID, languageIdForPath } from '../src/lib/editor/types';
import {
	createTokenState,
	tokenize,
	tokenizeLine,
	type FxcToken
} from '../src/lib/editor/tokenizer';
import {
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
} from '../src/lib/editor/wasm';

/** The `[kind, text]` pairs of a line, with whitespace elided. */
function kinds(line: string): [FxcToken, string][] {
	const { tokens } = tokenizeLine(line);
	return tokens.map((token) => [token.kind as FxcToken, line.slice(token.from, token.to)]);
}

/** The same, for a PRGM line. */
function prgmKinds(line: string): [PrgmToken, string][] {
	const { tokens } = tokenizePrgmLine(line);
	return tokens.map((token) => [token.kind as PrgmToken, line.slice(token.from, token.to)]);
}

/** Read a real `.fx` example, so the tokenizer is checked against shipped code. */
function readExample(name: string): string {
	return readFileSync(new URL(`../../examples/${name}`, import.meta.url), 'utf8');
}

/** A module stub, typed as the real client so the sources accept it. */
type Fx = NonNullable<WasmEditorContext['fx']>;

const bareView = (): EditorView => ({ state: { doc: Text.of(['']) } }) as unknown as EditorView;

describe('the .fxc tokenizer', () => {
	test('classifies keywords, names and punctuation', () => {
		expect(kinds('let x = 1;')).toEqual([
			['keyword', 'let'],
			['variable', 'x'],
			['operator', '='],
			['number', '1'],
			['punctuation', ';']
		]);
	});

	test('covers the whole keyword set', () => {
		const expected = [
			'let',
			'const',
			'free',
			'unsafe_free',
			'if',
			'else',
			'while',
			'for',
			'break',
			'goto',
			'label',
			'print',
			'fn',
			'return',
			'and',
			'or',
			'xor',
			'xnor'
		];
		for (const keyword of expected) {
			expect(kinds(`${keyword} x`)[0]).toEqual(['keyword', keyword]);
		}
	});

	test('distinguishes built-ins and user functions from variables', () => {
		expect(kinds('sqrt(2)')).toEqual([
			['builtin', 'sqrt'],
			['bracket', '('],
			['number', '2'],
			['bracket', ')']
		]);
		// A bare `i` is a variable; only `i()` is the imaginary unit.
		expect(kinds('i')[0]).toEqual(['variable', 'i']);
		expect(kinds('i()')[0]).toEqual(['builtin', 'i']);
		expect(kinds('sum_to(10)')[0]).toEqual(['function', 'sum_to']);
	});

	test('recognises the statement-only calculator keys', () => {
		const names = [
			'mplus',
			'mminus',
			'clrmemory',
			'clrstat',
			'freqon',
			'freqoff',
			'dt',
			'deg',
			'rad',
			'gra',
			'fix',
			'sci',
			'norm',
			'dec',
			'hex',
			'bin',
			'oct',
			'to_cartesian',
			'to_polar',
			're_im',
			'reg_lin',
			'reg_log',
			'reg_exp',
			'reg_pwr',
			'reg_inv',
			'reg_quad',
			'reg_abexp',
			'dms'
		];
		for (const name of names) {
			expect(kinds(`${name}(1)`)[0]).toEqual(['builtin', name]);
		}
	});

	test('colours the phys and stat namespaces through their dot', () => {
		expect(kinds('phys.h')).toEqual([
			['namespace', 'phys'],
			['punctuation', '.'],
			['constant', 'h']
		]);
		expect(kinds('stat.meanx')).toEqual([
			['namespace', 'stat'],
			['punctuation', '.'],
			['variable', 'meanx']
		]);
		// A data path is a property, not a variable.
		expect(kinds('config.offsets[0]')).toEqual([
			['variable', 'config'],
			['punctuation', '.'],
			['property', 'offsets'],
			['bracket', '['],
			['number', '0'],
			['bracket', ']']
		]);
	});

	test('a namespace does not leak past an operator', () => {
		expect(kinds('phys + config.x')).toEqual([
			['namespace', 'phys'],
			['operator', '+'],
			['variable', 'config'],
			['punctuation', '.'],
			['property', 'x']
		]);
	});

	test('accepts a display symbol after phys.', () => {
		expect(kinds('phys.R∞')).toEqual([
			['namespace', 'phys'],
			['punctuation', '.'],
			['constant', 'R∞']
		]);
	});

	test('reads every numeric spelling in the documentation', () => {
		const line = '0x1F 0b1010 0o17 1010b 17o 1Fh 2.5E-2 .5 1e10';
		expect(kinds(line)).toEqual([
			['number', '0x1F'],
			['number', '0b1010'],
			['number', '0o17'],
			['number', '1010b'],
			['number', '17o'],
			['number', '1Fh'],
			['number', '2.5E-2'],
			['number', '.5'],
			['number', '1e10']
		]);
		// `ah` is a name, not a hexadecimal literal.
		expect(kinds('ah')[0]).toEqual(['variable', 'ah']);
	});

	test('handles strings and both comment forms', () => {
		expect(kinds('"lib/x.fxc"')[0]).toEqual(['string', '"lib/x.fxc"']);
		expect(kinds('// to the end')).toEqual([['comment', '// to the end']]);
		expect(kinds('/* block */ x')).toEqual([
			['comment', '/* block */'],
			['variable', 'x']
		]);
	});

	test('carries an unterminated block comment across lines', () => {
		const first = tokenizeLine('/* start', createTokenState());
		expect(first.tokens).toEqual([{ from: 0, to: 8, kind: 'comment' }]);
		expect(first.state.inBlockComment).toBe(true);
		const second = tokenizeLine('still */ let x', first.state);
		expect(second.tokens[0]).toEqual({ from: 0, to: 8, kind: 'comment' });
		expect(second.state.inBlockComment).toBe(false);
		expect(second.tokens.slice(1).map((token) => token.kind)).toEqual(['keyword', 'variable']);
	});

	test('colours directives and the mode name', () => {
		expect(kinds('#mode CMPLX')).toEqual([
			['directive', '#mode'],
			['mode', 'CMPLX']
		]);
		expect(kinds('#mode=BASE')).toEqual([
			['directive', '#mode'],
			['operator', '='],
			['mode', 'BASE']
		]);
		expect(kinds('#include "lib/x.fxc"')).toEqual([
			['directive', '#include'],
			['string', '"lib/x.fxc"']
		]);
		expect(kinds('#data config = 3;')[0]).toEqual(['directive', '#data']);
	});

	test('colours the calculator glyph operators', () => {
		expect(kinds('a × b → c')).toEqual([
			['variable', 'a'],
			['operator', '×'],
			['variable', 'b'],
			['operator', '→'],
			['variable', 'c']
		]);
		expect(kinds('π ≠ 3.14')).toEqual([
			['constant', 'π'],
			['operator', '≠'],
			['number', '3.14']
		]);
	});

	test('tokenizes every bundled example without gaps or stalls', () => {
		for (const example of examples) {
			const sourceLines = example.source.split('\n');
			const tokenized = tokenize(example.source);
			expect(tokenized).toHaveLength(sourceLines.length);
			for (const { line, tokens } of tokenized) {
				const text = sourceLines[line];
				let cursor = 0;
				for (const token of tokens) {
					expect(token.from).toBeGreaterThanOrEqual(cursor);
					expect(text.slice(cursor, token.from).trim()).toBe('');
					cursor = token.to;
				}
				expect(text.slice(cursor).trim()).toBe('');
			}
		}
	});
});

describe('LSP → CodeMirror mapping', () => {
	test('maps diagnostic severities', () => {
		expect(severityFor(DiagnosticSeverity.Error)).toBe('error');
		expect(severityFor(DiagnosticSeverity.Warning)).toBe('warning');
		expect(severityFor(DiagnosticSeverity.Information)).toBe('info');
		expect(severityFor(DiagnosticSeverity.Hint)).toBe('hint');
		expect(severityFor(undefined)).toBe('error');
	});

	test('maps completion kinds to CodeMirror types', () => {
		expect(completionTypeFor(CompletionItemKind.Method)).toBe('method');
		expect(completionTypeFor(CompletionItemKind.Function)).toBe('function');
		expect(completionTypeFor(CompletionItemKind.Variable)).toBe('variable');
		expect(completionTypeFor(CompletionItemKind.Constant)).toBe('constant');
		expect(completionTypeFor(CompletionItemKind.Keyword)).toBe('keyword');
		expect(completionTypeFor(CompletionItemKind.Operator)).toBe('operator');
		expect(completionTypeFor(CompletionItemKind.Struct)).toBe('type');
		expect(completionTypeFor(CompletionItemKind.Text)).toBe('text');
		expect(completionTypeFor(undefined)).toBeUndefined();
	});

	test('turns a snippet completion into a CodeMirror snippet', () => {
		const item: CompletionItem = {
			label: 'sqrt(',
			kind: CompletionItemKind.Function,
			detail: 'built-in: 1 argument',
			insertText: 'sqrt($0)',
			insertTextFormat: InsertTextFormat.Snippet,
			documentation: { kind: 'markdown', value: 'square root' }
		};
		const completion = toCodeMirrorCompletion(item);
		expect(completion.label).toBe('sqrt(');
		expect(completion.type).toBe('function');
		expect(completion.detail).toBe('built-in: 1 argument');
		expect(completion.info).toBe('square root');
		expect(typeof completion.apply).toBe('function');
	});

	test('uses insertText as the apply text when it differs from the label', () => {
		const completion = toCodeMirrorCompletion({ label: '#mode', insertText: '#mode ' });
		expect(completion.apply).toBe('#mode ');
		// Identical text leaves CodeMirror's default (the label) in place.
		expect(toCodeMirrorCompletion({ label: 'let' }).apply).toBeUndefined();
	});

	test('maps an LSP position to a document offset, clamped', () => {
		const doc = Text.of(['abc', 'defgh']);
		expect(toOffset(doc, { line: 0, character: 2 })).toBe(2);
		expect(toOffset(doc, { line: 1, character: 3 })).toBe(7);
		expect(toOffset(doc, { line: 1, character: 99 })).toBe(9);
		expect(toOffset(doc, { line: 0, character: -5 })).toBe(0);
		expect(toOffset(doc, { line: 99, character: 0 })).toBe(4);
	});

	test('maps an LSP diagnostic to a CodeMirror range', () => {
		const diagnostic: Diagnostic = {
			range: { start: { line: 0, character: 0 }, end: { line: 0, character: 3 } },
			severity: DiagnosticSeverity.Error,
			message: 'boom',
			source: 'fx-50FH II'
		};
		expect(toCodeMirrorDiagnostic(diagnostic, Text.of(['abc']))).toMatchObject({
			from: 0,
			to: 3,
			severity: 'error',
			message: 'boom',
			source: 'fx-50FH II'
		});
	});

	test('flattens the shapes hover contents can take', () => {
		expect(hoverText(null)).toBeNull();
		expect(hoverText({ contents: 'plain' })).toBe('plain');
		expect(hoverText({ contents: { kind: 'markdown', value: '**bold**' } })).toBe('**bold**');
		const hover: Hover = {
			contents: [{ language: 'fxc', value: 'first' }, 'second']
		};
		expect(hoverText(hover)).toBe('first\n\nsecond');
	});
});

describe('wasm-backed sources', () => {
	const diagnostic: Diagnostic = {
		range: { start: { line: 0, character: 0 }, end: { line: 0, character: 2 } },
		severity: DiagnosticSeverity.Error,
		message: 'unknown function'
	};

	test('a lint source is a no-op without a module', async () => {
		const source = createLintSource(() => ({ fx: null }));
		expect(await source(bareView())).toEqual([]);
	});

	test('a lint source maps the module diagnostics', async () => {
		const fx = {
			diagnostics: () => ({ ok: true, language: 'fxc', diagnostics: [diagnostic] })
		} as unknown as Fx;
		const view = { state: { doc: Text.of(['fn main() {}']) } } as unknown as EditorView;
		const result = await createLintSource(() => ({ fx, path: 'main.fxc' }))(view);
		expect(result).toHaveLength(1);
		expect(result[0]).toMatchObject({
			from: 0,
			to: 2,
			severity: 'error',
			message: 'unknown function'
		});
	});

	test('a completion source is a no-op without a module', async () => {
		const state = EditorState.create({ doc: 'sq' });
		const context = new CompletionContext(state, 2, true);
		expect(await createCompletionSource(() => ({ fx: null }))(context)).toBeNull();
	});

	test('a completion source maps the module items', async () => {
		const fx = {
			completions: () => ({
				ok: true,
				language: 'fxc',
				items: [
					{
						label: 'sqrt(',
						kind: CompletionItemKind.Function,
						insertText: 'sqrt($0)',
						insertTextFormat: InsertTextFormat.Snippet
					}
				]
			})
		} as unknown as Fx;
		const state = EditorState.create({ doc: 'sq' });
		const context = new CompletionContext(state, 2, true);
		const result = await createCompletionSource(() => ({ fx }))(context);
		expect(result).not.toBeNull();
		expect(result?.from).toBe(0);
		expect(result?.options[0].label).toBe('sqrt(');
		expect(result?.options[0].type).toBe('function');
	});

	test('a hover source is a no-op without a module', async () => {
		const source = createHoverSource(() => ({ fx: null }));
		expect(await source(bareView(), 0, 1)).toBeNull();
	});

	test('a hover source asks for the word under the pointer', async () => {
		const fx = {
			hover: () => ({
				ok: true,
				hover: {
					contents: { kind: 'markdown', value: '**sqrt**' },
					range: { start: { line: 0, character: 0 }, end: { line: 0, character: 4 } }
				}
			})
		} as unknown as Fx;
		const view = { state: { doc: Text.of(['sqrt(2)']) } } as unknown as EditorView;
		const result = await createHoverSource(() => ({ fx }))(view, 1, 1);
		expect(result).not.toBeNull();
		const tooltip = result as { pos: number; end?: number; create: unknown };
		expect(tooltip.pos).toBe(0);
		expect(tooltip.end).toBe(4);
		expect(typeof tooltip.create).toBe('function');
	});
});

describe('language routing', () => {
	test('derives the language id from the path, as Language::from_path does', () => {
		expect(languageIdForPath('main.fxc')).toBe(FXC_LANGUAGE_ID);
		expect(languageIdForPath('lib/geometry.FXC')).toBe(FXC_LANGUAGE_ID);
		expect(languageIdForPath('file:///tmp/a.fxc')).toBe(FXC_LANGUAGE_ID);
		expect(languageIdForPath('prog.fx')).toBe(PRGM_LANGUAGE_ID);
		expect(languageIdForPath('PROG.FX')).toBe(PRGM_LANGUAGE_ID);
		expect(languageIdForPath('notes.txt')).toBe(PRGM_LANGUAGE_ID);
		expect(languageIdForPath('no-extension')).toBe(PRGM_LANGUAGE_ID);
	});

	test('the same routing picks the editor language support', () => {
		expect(languageSupportForPath('prog.fx')).toBe(prgmLanguageSupport);
		expect(languageSupportForPath('main.fxc')).toBe(fxcLanguageSupport);
	});
});

describe('the PRGM tokenizer', () => {
	test('classifies the program statements', () => {
		expect(prgmKinds('If X Then')).toEqual([
			['keyword', 'If'],
			['variable', 'X'],
			['keyword', 'Then']
		]);
		expect(prgmKinds('For 1→X To A')).toEqual([
			['keyword', 'For'],
			['number', '1'],
			['operator', '→'],
			['variable', 'X'],
			['keyword', 'To'],
			['variable', 'A']
		]);
		expect(prgmKinds('WhileEnd')).toEqual([['keyword', 'WhileEnd']]);
		expect(prgmKinds('Dsz X')).toEqual([
			['keyword', 'Dsz'],
			['variable', 'X']
		]);
		expect(prgmKinds('Isz')).toEqual([['keyword', 'Isz']]);
	});

	test('covers the whole program keyword set', () => {
		const keywords = [
			'If',
			'Then',
			'Else',
			'IfEnd',
			'For',
			'To',
			'Step',
			'Next',
			'While',
			'WhileEnd',
			'Break',
			'Goto',
			'Lbl',
			'Dsz',
			'Isz'
		];
		for (const keyword of keywords) {
			expect(prgmKinds(keyword)[0]).toEqual(['keyword', keyword]);
		}
	});

	test('classifies display, assignment and comparison keys', () => {
		expect(prgmKinds('B×X→B')).toEqual([
			['variable', 'B'],
			['operator', '×'],
			['variable', 'X'],
			['operator', '→'],
			['variable', 'B']
		]);
		expect(prgmKinds('D≥0⇒Goto 1')).toEqual([
			['variable', 'D'],
			['operator', '≥'],
			['number', '0'],
			['operator', '⇒'],
			['keyword', 'Goto'],
			['number', '1']
		]);
		expect(prgmKinds('B◢')).toEqual([
			['variable', 'B'],
			['operator', '◢']
		]);
		expect(prgmKinds('A≠B')).toEqual([
			['variable', 'A'],
			['operator', '≠'],
			['variable', 'B']
		]);
	});

	test('classifies the setup and base-n words', () => {
		const words = [
			'Dec',
			'Hex',
			'Bin',
			'Oct',
			'Fix',
			'Sci',
			'Norm',
			'Deg',
			'Rad',
			'Gra',
			'ClrMemory',
			'ClrStat',
			'FreqOn',
			'FreqOff'
		];
		for (const word of words) {
			expect(prgmKinds(word)[0]).toEqual(['keyword', word]);
		}
	});

	test('classifies the M+ / M- memory keys, but not as arithmetic', () => {
		expect(prgmKinds('M+')).toEqual([['keyword', 'M+']]);
		expect(prgmKinds('M-')).toEqual([['keyword', 'M-']]);
		expect(prgmKinds('M+3')).toEqual([
			['variable', 'M'],
			['operator', '+'],
			['number', '3']
		]);
	});

	test('classifies functions, including Pol( and Rec(', () => {
		expect(prgmKinds('Pol(')).toEqual([
			['function', 'Pol'],
			['bracket', '(']
		]);
		expect(prgmKinds('Rec(1,2)')).toEqual([
			['function', 'Rec'],
			['bracket', '('],
			['number', '1'],
			['punctuation', ','],
			['number', '2'],
			['bracket', ')']
		]);
		expect(prgmKinds('√(D)')[0]).toEqual(['function', '√']);
	});

	test('colours the stat. and phys. namespaces through their dot', () => {
		expect(prgmKinds('stat.meanx')).toEqual([
			['namespace', 'stat'],
			['punctuation', '.'],
			['statistic', 'meanx']
		]);
		expect(prgmKinds('phys.h')).toEqual([
			['namespace', 'phys'],
			['punctuation', '.'],
			['constant', 'h']
		]);
	});

	test('reads the base literals', () => {
		expect(prgmKinds('FFh 1010b 17o 42d')).toEqual([
			['number', 'FFh'],
			['number', '1010b'],
			['number', '17o'],
			['number', '42d']
		]);
		expect(prgmKinds('2B')).toEqual([
			['number', '2'],
			['variable', 'B']
		]);
		expect(prgmKinds('4disp')).toEqual([
			['number', '4'],
			['operator', 'disp']
		]);
	});

	test('classifies Ran# and the postfix keys', () => {
		expect(prgmKinds('Ran#')).toEqual([['function', 'Ran#']]);
		expect(prgmKinds('B²')).toEqual([
			['variable', 'B'],
			['postfix', '²']
		]);
		expect(prgmKinds('A³')).toEqual([
			['variable', 'A'],
			['postfix', '³']
		]);
		expect(prgmKinds('A⁻¹')).toEqual([
			['variable', 'A'],
			['postfix', '⁻¹']
		]);
		expect(prgmKinds('A!')).toEqual([
			['variable', 'A'],
			['postfix', '!']
		]);
		expect(prgmKinds('A%')).toEqual([
			['variable', 'A'],
			['postfix', '%']
		]);
	});

	test('colours the glyphs and the input key', () => {
		expect(prgmKinds('A×B÷C')).toEqual([
			['variable', 'A'],
			['operator', '×'],
			['variable', 'B'],
			['operator', '÷'],
			['variable', 'C']
		]);
		expect(prgmKinds('π∠2')).toEqual([
			['constant', 'π'],
			['operator', '∠'],
			['number', '2']
		]);
		expect(prgmKinds('∛')).toEqual([['function', '∛']]);
		expect(prgmKinds('?→A')).toEqual([
			['operator', '?'],
			['operator', '→'],
			['variable', 'A']
		]);
	});

	test('handles comments and the #mode directive', () => {
		expect(prgmKinds('// to the end')).toEqual([['comment', '// to the end']]);
		expect(prgmKinds('#mode CMPLX')).toEqual([
			['directive', '#mode'],
			['mode', 'CMPLX']
		]);
		expect(prgmKinds('#mode=BASE')).toEqual([
			['directive', '#mode'],
			['operator', '='],
			['mode', 'BASE']
		]);
	});

	test('does not claim .fxc-only constructs as PRGM syntax', () => {
		expect(prgmKinds('let x = 1;').map(([kind]) => kind)).not.toContain('keyword');
		expect(prgmKinds('fn main() {}').map(([kind]) => kind)).not.toContain('keyword');
		expect(prgmKinds('#include "x.fxc"')[0]).toEqual(['operator', '#']);
		expect(prgmKinds('#include')).not.toContainEqual(['directive', '#include']);
	});

	test('tokenizes every .fx example without gaps or stalls', () => {
		for (const name of [
			'factorial.fx',
			'fibonacci.fx',
			'gcd.fx',
			'quadratic.fx',
			'complex_quadratic.fx',
			'statistics.fx'
		]) {
			const source = readExample(name);
			const sourceLines = source.split('\n');
			const tokenized = tokenizePrgm(source);
			expect(tokenized).toHaveLength(sourceLines.length);
			for (const { line, tokens } of tokenized) {
				const text = sourceLines[line];
				let cursor = 0;
				for (const token of tokens) {
					expect(token.from).toBeGreaterThanOrEqual(cursor);
					expect(text.slice(cursor, token.from).trim()).toBe('');
					cursor = token.to;
				}
				expect(text.slice(cursor).trim()).toBe('');
			}
		}
	});

	test('a .fx example yields PRGM tokens, not .fxc ones', () => {
		const factorial = readExample('factorial.fx');
		const found = new Set(
			tokenizePrgm(factorial).flatMap(({ tokens }) => tokens.map((token) => token.kind))
		);
		expect(found).toContain('keyword');
		expect(found).toContain('variable');
		expect(found).toContain('number');
		expect(found).toContain('operator');
		expect(found).toContain('comment');
		expect(found).not.toContain('directive');

		const complex = readExample('complex_quadratic.fx');
		const lines = complex.split('\n');
		const tokenized = tokenizePrgm(complex);
		expect(
			tokenized[4].tokens.map((token) => [token.kind, lines[4].slice(token.from, token.to)])
		).toEqual([
			['directive', '#mode'],
			['mode', 'CMPLX']
		]);
		expect(tokenized[6].tokens.map((token) => token.kind)).toEqual([
			'variable',
			'postfix',
			'operator',
			'number',
			'variable',
			'operator',
			'variable',
			'punctuation'
		]);
	});
});

describe('wasm-backed sources choose the language from the path', () => {
	test('completions ask for the language of the path', async () => {
		const seen: string[] = [];
		const fx = {
			completions: (language: string) => {
				seen.push(language);
				return { ok: true, language, items: [] };
			}
		} as unknown as Fx;
		const state = EditorState.create({ doc: 'sq' });
		const context = new CompletionContext(state, 2, true);
		await createCompletionSource(() => ({ fx, path: 'prog.fx' }))(context);
		await createCompletionSource(() => ({ fx, path: 'main.fxc' }))(context);
		expect(seen).toEqual([PRGM_LANGUAGE_ID, FXC_LANGUAGE_ID]);
	});

	test('diagnostics and hover ask for the language of the path', async () => {
		const seen: string[] = [];
		const fx = {
			diagnostics: (options: { language?: string }) => {
				seen.push(options.language ?? '');
				return { ok: true, language: options.language, diagnostics: [] };
			},
			hover: (options: { language?: string }) => {
				seen.push(options.language ?? '');
				return { ok: true, hover: null };
			}
		} as unknown as Fx;
		const view = { state: { doc: Text.of(['1→A']) } } as unknown as EditorView;
		await createLintSource(() => ({ fx, path: 'prog.fx' }))(view);
		await createHoverSource(() => ({ fx, path: 'main.fxc' }))(view, 0, 1);
		expect(seen).toEqual([PRGM_LANGUAGE_ID, FXC_LANGUAGE_ID]);
	});

	test('an absent path falls back to the module entry, main.fxc', async () => {
		let seen: string | undefined;
		const fx = {
			completions: (language: string) => {
				seen = language;
				return { ok: true, language, items: [] };
			}
		} as unknown as Fx;
		const state = EditorState.create({ doc: 'sq' });
		const context = new CompletionContext(state, 2, true);
		await createCompletionSource(() => ({ fx }))(context);
		expect(seen).toBe(FXC_LANGUAGE_ID);
	});
});
