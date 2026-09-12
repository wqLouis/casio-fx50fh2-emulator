/**
 * The PRGM tokenizer, with no CodeMirror import.
 *
 * `language.ts` adapts this to a CodeMirror `StreamLanguage`, exactly as it
 * does for `.fxc`. Keeping the state machine here — over the shared
 * `TokenStream` rather than CodeMirror's `StringStream` — is what lets the
 * token decisions be unit-tested over a plain string.
 *
 * The token set is checked against `docs/LANGUAGE.md`, the interpreter's own
 * lexer (`src/lexer.rs`) and every `.fx` program under `examples/`. It is
 * deliberately a *highlighter*, not the lexer: `docs/LANGUAGE.md` says the
 * calculator has no indirect addressing and only a fixed keyboard, so this
 * colours the keys that exist and leaves everything else alone.
 *
 * Two things are deliberately absent, because the calculator does not have
 * them and colouring them would be a lie:
 *
 *   * `#include`/`#data` — PRGM's only directive is `#mode`, so any other
 *     `#word` is emitted as an operator plus an ordinary name, not a directive;
 *   * `fn`/`let`/`const`/`return`/`print` — those are `.fxc` grammar words and
 *     become plain identifiers here.
 */

import { LineStream, type TokenStream } from './tokenizer';

/** What a run of PRGM text is, in the editor's vocabulary. */
export type PrgmToken =
	| 'keyword'
	| 'directive'
	| 'mode'
	| 'function'
	| 'namespace'
	| 'constant'
	| 'variable'
	| 'statistic'
	| 'number'
	| 'comment'
	| 'operator'
	| 'postfix'
	| 'bracket'
	| 'punctuation';

/**
 * Cross-token state.
 *
 * PRGM has no block comments, so nothing has to survive a newline; the fields
 * track only the current line so `phys.` / `stat.` members and the name after
 * `#mode` are coloured, then dropped at the next line start.
 */
export interface PrgmTokenState {
	/** The previous significant token was `.`, opening a namespace member. */
	afterDot: boolean;
	/** The namespace word (`phys`/`stat`) before that dot. */
	namespace: 'phys' | 'stat' | null;
	/** The previous significant token was `#mode`, so this word is a mode. */
	afterMode: boolean;
}

/** A fresh state, at the start of a document. */
export function createPrgmTokenState(): PrgmTokenState {
	return { afterDot: false, namespace: null, afterMode: false };
}

/**
 * Program, setup, base-n and regression words.
 *
 * These are the calculator keys that act on control flow or on the machine
 * itself. `Dsz`/`Isz` are included even though this implementation's lexer does
 * not support them yet: the task's token set names them, and the hardware
 * keyboard has them.
 */
const KEYWORDS = new Set([
	// control flow
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
	'Isz',
	// environment setup
	'Deg',
	'Rad',
	'Gra',
	'Fix',
	'Sci',
	'Norm',
	'Dec',
	'Hex',
	'Bin',
	'Oct',
	'ClrMemory',
	'ClrStat',
	'FreqOn',
	'FreqOff',
	'DT',
	'Re⇔Im',
	// regression models (REG)
	'Lin',
	'Log',
	'Exp',
	'Pwr',
	'Inv',
	'Quad',
	'AB-Exp'
]);

/**
 * Prefix functions — the calculator keys that take an argument, whether they
 * are written with parentheses (`Abs(`) or as a glyph (`√`). `x√` and `10^`
 * are multi-character keys, so they are matched before the word scanner.
 */
const FUNCTIONS = new Set([
	'sin',
	'cos',
	'tan',
	'asin',
	'acos',
	'atan',
	'sinh',
	'cosh',
	'tanh',
	'asinh',
	'acosh',
	'atanh',
	'log',
	'ln',
	'sqrt',
	'cbrt',
	'Abs',
	'Pol',
	'Rec',
	'Rnd',
	'arg',
	'Conjg',
	'Not',
	'Neg'
]);

/** `π`/`pi`, `e`, `i`, and the 40 scientific constants by name or symbol. */
const CONSTANTS = new Set([
	'π',
	'pi',
	'e',
	'i',
	// scientific constants, ASCII names
	'mp',
	'mn',
	'me',
	'mmu',
	'a0',
	'h',
	'muN',
	'muB',
	'hbar',
	'alpha',
	're',
	'lc',
	'gp',
	'lcp',
	'lcn',
	'Rinf',
	'u',
	'mup',
	'mue',
	'mun',
	'mumu',
	'F',
	'eq',
	'NA',
	'k',
	'Vm',
	'R',
	'C0',
	'C1',
	'C2',
	'sigma',
	'eps0',
	'mu0',
	'phi0',
	'g',
	'G0',
	'Z0',
	'tK',
	'G',
	'atm',
	// scientific constants, display symbols
	'mμ',
	'μN',
	'μB',
	'ħ',
	'α',
	'λc',
	'γp',
	'λcp',
	'λcn',
	'R∞',
	'μp',
	'μe',
	'μn',
	'μμ',
	'σ',
	'ε0',
	'μ0',
	'φ0',
	't'
]);

/** The seven memories plus `Ans`. */
const VARIABLES = new Set(['A', 'B', 'C', 'D', 'X', 'Y', 'M', 'Ans']);

/** Statistical values, by ASCII alias or display glyph. */
const STATISTICS = new Set([
	'n',
	'sumx',
	'sumx2',
	'sumy',
	'sumy2',
	'sumxy',
	'meanx',
	'meany',
	'sigmax',
	'sigmay',
	'sx',
	'sy',
	'minx',
	'minX',
	'maxx',
	'maxX',
	'miny',
	'minY',
	'maxy',
	'maxY',
	'rega',
	'regA',
	'regb',
	'regB',
	'regc',
	'regC',
	'regr',
	'regR',
	'Σx',
	'Σx²',
	'Σy',
	'Σy²',
	'Σxy',
	'x̄',
	'ȳ',
	'σx',
	'σy'
]);

/** Word operators: the base-n words, `disp`, and the permutations keys. */
const OPERATOR_WORDS = new Set(['nPr', 'nCr', 'and', 'or', 'xor', 'xnor', 'div', 'disp']);

/** The multi-character symbolic keys, longest first as needed. */
const SYMBOL_KEYS: [string, PrgmToken][] = [
	// complex display formats
	['▶a+b𝑖', 'operator'],
	['▶a+b𝒾', 'operator'],
	['▶a+bi', 'operator'],
	['▶r∠θ', 'operator'],
	['>a+bi', 'operator'],
	['>rangle', 'operator'],
	// postfix power keys
	['^-1', 'postfix'],
	['⁻¹', 'postfix'],
	['^2', 'postfix'],
	['^3', 'postfix'],
	// ASCII aliases for the glyph operators
	['->', 'operator'],
	['=>', 'operator'],
	['<=', 'operator'],
	['>=', 'operator'],
	['<>', 'operator']
];

/** Suffixes that tag an integer literal with its base: `FFh`, `1010b`, `17o`, `42d`. */
const BASE_SUFFIXES = 'hHbod';

/** Decimal literal; the exponent is optional and needs at least one digit. */
const DECIMAL = /^(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?/;

/** Does `text` end in a suffix whose prefix is valid for that base? */
function isValidBaseLiteral(text: string): boolean {
	const suffix = text[text.length - 1];
	const digits = text.slice(0, -1);
	switch (suffix) {
		case 'h':
		case 'H':
			return /^[0-9A-Fa-f]+$/.test(digits);
		case 'b':
			return /^[01]+$/.test(digits);
		case 'o':
			return /^[0-7]+$/.test(digits);
		case 'd':
			return /^[0-9]+$/.test(digits);
		default:
			return false;
	}
}

/**
 * The length of a base literal at the start of `rest`, or 0.
 *
 * This reproduces `Lexer::try_base_literal` in `src/lexer.rs`: scan hex digits,
 * accept a trailing suffix character if either the next character is a suffix
 * or the run is at least two characters and ended in one, reject `4disp` (a
 * tagged literal may not run into an identifier), and reject a suffix whose
 * prefix is not valid for that base (`Ab` is not binary).
 */
function baseLiteralLength(rest: string): number {
	let end = 0;
	while (end < rest.length && /[0-9A-Fa-f]/.test(rest[end])) end++;
	if (end === 0) return 0;

	let candidate: number;
	if (end < rest.length && BASE_SUFFIXES.includes(rest[end])) {
		candidate = end + 1;
	} else if (end >= 2 && BASE_SUFFIXES.includes(rest[end - 1])) {
		candidate = end;
	} else {
		return 0;
	}

	const after = rest[candidate];
	if (after !== undefined && /[A-Za-z0-9_]/.test(after)) return 0;
	return isValidBaseLiteral(rest.slice(0, candidate)) ? candidate : 0;
}

/** Forgetting a namespace after a non-dot token keeps `stat + config.x` honest. */
function resetContext(state: PrgmTokenState): void {
	state.namespace = null;
	state.afterDot = false;
}

function isWordStart(ch: string): boolean {
	return /^\p{L}$/u.test(ch);
}

function isWordChar(ch: string): boolean {
	// Combining marks are included so the display spelling of `x̄` is one word;
	// ASCII digits are included so `C0`, `sumx2` and `a0` stay whole. The digits
	// are deliberately ASCII-only: `\p{N}` would swallow the superscripts in
	// `B²`/`Σx²` and hide the postfix keys.
	return /^[\p{L}\p{M}0-9_]$/u.test(ch);
}

function classifyWord(word: string, state: PrgmTokenState): PrgmToken {
	if (state.afterDot) {
		const namespace = state.namespace;
		resetContext(state);
		if (namespace === 'phys') return 'constant';
		if (namespace === 'stat') return 'statistic';
		return 'variable';
	}
	if (state.afterMode) {
		state.afterMode = false;
		return 'mode';
	}

	state.namespace = null;
	if (word === 'phys' || word === 'stat') {
		state.namespace = word;
		return 'namespace';
	}
	if (KEYWORDS.has(word)) return 'keyword';
	if (FUNCTIONS.has(word)) return 'function';
	if (STATISTICS.has(word)) return 'statistic';
	if (CONSTANTS.has(word)) return 'constant';
	if (VARIABLES.has(word)) return 'variable';
	if (OPERATOR_WORDS.has(word)) return 'operator';
	return 'variable';
}

/**
 * Read one token, advancing the stream. `null` means the run is whitespace and
 * should not be coloured. The order of the checks follows the lexer: multi-key
 * symbols, then base literals (so `FFh` is one number), then words, then
 * numbers, then single characters.
 */
export function prgmToken(stream: TokenStream, state: PrgmTokenState): PrgmToken | null {
	const start = stream.pos;

	if (stream.sol()) {
		state.afterDot = false;
		state.namespace = null;
		state.afterMode = false;
	}

	if (stream.eatSpace()) return null;

	// PRGM has only the `//` comment form.
	if (stream.match('//')) {
		stream.skipToEnd();
		return 'comment';
	}

	// `#mode` is the only directive PRGM has. Every other `#word` is left alone
	// (the `#` as an operator, the name as an ordinary word) so that a stray
	// `#include` from a `.fxc` habit is not painted as valid PRGM.
	if (stream.peek() === '#') {
		if (stream.match('#mode', true, true)) {
			state.afterMode = true;
			return 'directive';
		}
		stream.next();
		return 'operator';
	}

	// Single calculator keys that begin with a symbol rather than a letter.
	if (stream.match('Re⇔Im')) return 'keyword';
	if (stream.match('°′″')) return 'operator';
	if (stream.match('Ran#')) return 'function';
	if (stream.match('10^')) return 'function';
	if (stream.match('x√')) return 'function';

	const baseLength = baseLiteralLength(stream.string.slice(stream.pos));
	if (baseLength > 0) {
		for (let i = 0; i < baseLength; i++) stream.next();
		resetContext(state);
		return 'number';
	}

	const first = stream.peek();
	if (first !== undefined && isWordStart(first)) {
		// `M+`/`M-` are standalone keys only when not followed by an operand;
		// `M+3` is arithmetic on the `M` memory (`src/lexer.rs`).
		if (stream.match('M+', false) || stream.match('M-', false)) {
			const after = stream.string[stream.pos + 2];
			if (after === undefined || /[\s:◢]/.test(after)) {
				stream.next();
				stream.next();
				resetContext(state);
				return 'keyword';
			}
		}
		stream.next();
		stream.eatWhile(isWordChar);
		return classifyWord(stream.string.slice(start, stream.pos), state);
	}

	if (stream.match(DECIMAL)) {
		resetContext(state);
		return 'number';
	}

	for (const [text, kind] of SYMBOL_KEYS) {
		if (stream.match(text)) {
			resetContext(state);
			return kind;
		}
	}

	const ch = stream.peek();
	if (ch === undefined) return null;
	stream.next();

	// A lone dot is punctuation, but it also opens a member name, so it must
	// not clear the namespace the left side established.
	if (ch === '.') {
		state.afterDot = true;
		return 'punctuation';
	}

	resetContext(state);
	if (ch === '√' || ch === '∛') return 'function';
	if (ch === '²' || ch === '³' || ch === '!' || ch === '%') return 'postfix';
	if (ch === '(' || ch === ')') return 'bracket';
	if (ch === ',' || ch === ':' || ch === ';') return 'punctuation';
	return 'operator';
}

/** One run of text and what it is. Whitespace is not represented. */
export interface PrgmTokenSpan {
	from: number;
	to: number;
	kind: PrgmToken | null;
}

/** Tokenize one line, carrying `state` across calls. */
export function tokenizePrgmLine(
	line: string,
	state: PrgmTokenState = createPrgmTokenState()
): { tokens: PrgmTokenSpan[]; state: PrgmTokenState } {
	const stream = new LineStream(line);
	const tokens: PrgmTokenSpan[] = [];
	while (!stream.eol()) {
		const from = stream.pos;
		const kind = prgmToken(stream, state);
		if (stream.pos === from) {
			// A tokenizer bug must not become an infinite loop in a test.
			stream.next();
			continue;
		}
		if (kind) tokens.push({ from, to: stream.pos, kind });
	}
	return { tokens, state };
}

/** Tokenize a whole document, one line at a time. */
export function tokenizePrgm(
	source: string,
	state: PrgmTokenState = createPrgmTokenState()
): { line: number; tokens: PrgmTokenSpan[] }[] {
	return source.split('\n').map((line, index) => {
		const result = tokenizePrgmLine(line, state);
		state = result.state;
		return { line: index, tokens: result.tokens };
	});
}
