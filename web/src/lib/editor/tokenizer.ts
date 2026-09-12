/**
 * The `.fxc` tokenizer, with no CodeMirror import.
 *
 * `language.ts` adapts this to a CodeMirror `StreamLanguage`. Keeping the state
 * machine here — over a tiny stream interface rather than `StringStream` — is
 * what lets the token decisions be unit-tested without a DOM: `tokenizeLine`
 * drives the exact same `token` function over a plain string.
 *
 * The token set is checked against `docs/FXC.md` and every program under
 * `examples/`. It is deliberately a *highlighter*, not the transpiler's lexer:
 * the calculator's glyph operators and the PRGM-style base suffixes (`1010b`,
 * `FFh`) are coloured even though only the ASCII spellings are valid `.fxc`.
 * A wrong token here costs a colour, never a compile.
 */

/** What a run of text is, in the editor's vocabulary. */
export type FxcToken =
	| 'keyword'
	| 'directive'
	| 'mode'
	| 'namespace'
	| 'builtin'
	| 'constant'
	| 'property'
	| 'variable'
	| 'function'
	| 'number'
	| 'string'
	| 'comment'
	| 'operator'
	| 'bracket'
	| 'punctuation';

/**
 * Cross-line state. Block comments are the only thing that survives a newline;
 * everything else is reset when a line starts so a stray `phys` or `#mode`
 * cannot colour the next line.
 */
export interface FxcTokenState {
	inBlockComment: boolean;
	/** The previous significant token was `.`, so this word is a member. */
	afterDot: boolean;
	/** The namespace word before a `.`: how the member is coloured. */
	namespace: 'phys' | 'stat' | null;
	/** The previous significant token was `#mode`, so this word is a mode. */
	afterMode: boolean;
}

/**
 * The subset of `StringStream` this tokenizer needs.
 *
 * `@codemirror/language`'s `StringStream` satisfies it structurally, so the
 * same function serves the editor and the tests.
 */
export interface TokenStream {
	string: string;
	pos: number;
	eol(): boolean;
	sol(): boolean;
	peek(): string | undefined;
	next(): string | void;
	eatWhile(match: string | RegExp | ((ch: string) => boolean)): boolean;
	eatSpace(): boolean;
	skipToEnd(): void;
	skipTo(ch: string): boolean | void;
	match(
		pattern: string | RegExp,
		consume?: boolean,
		caseInsensitive?: boolean
	): boolean | RegExpMatchArray | null;
}

/** A fresh state, at the start of a document. */
export function createTokenState(): FxcTokenState {
	return { inBlockComment: false, afterDot: false, namespace: null, afterMode: false };
}

/**
 * Grammar words. `phys` and `stat` are handled as namespaces, not keywords, so
 * the word after their dot can be coloured differently.
 */
const KEYWORDS = new Set([
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
]);

/**
 * Everything callable: the value built-ins from `fx-transpiler`'s table, plus
 * the statement-only calculator keys the parser recognises (`deg`, `fix`,
 * `dt`, …) and `input`.
 */
const BUILTINS = new Set([
	// value built-ins
	'sqrt',
	'cbrt',
	'root',
	'pow10',
	'exp',
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
	'abs',
	'rnd',
	'inv',
	'sqr',
	'cube',
	'fact',
	'pct',
	'frac',
	'npr',
	'ncr',
	'pol',
	'rec',
	'arg',
	'conjg',
	'rep',
	'imp',
	'polar',
	'dms',
	'not',
	'neg',
	'ran',
	'i',
	'ans',
	'mvalue',
	'input',
	// statement-only calculator keys
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
	'clrmemory',
	'clrstat',
	'freqon',
	'freqoff',
	'mplus',
	'mminus',
	'dt'
]);

/** `pi`/`e`, and the JSON scalars that appear inside `#data`/`#tests`. */
const CONSTANTS = new Set(['pi', 'e', 'true', 'false', 'null']);

const BRACKETS = new Set(['(', ')', '{', '}', '[', ']']);
const PUNCTUATION = new Set([';', ',']);

/** Operators that are more than one code unit. */
const LONG_OPERATORS = new Set(['**', '==', '!=', '<=', '>=', '=>', '->', '<>', '&&', '||', '⁻¹']);

/** The calculator's own glyph operators, which the docs print beside the ASCII. */
const GLYPH_OPERATORS = new Set(['×', '÷', '≠', '≤', '≥', '∠', '√', '→', '⇒', '²', '³', '!', '%']);

/**
 * Prefix base literals, PRGM-style suffix literals, and decimals — in that
 * order, so `0x1F` is not read as `0` then `x1F`, and `1010b` is not read as
 * `1010` then the name `b`.
 */
const NUMBER =
	/^(?:0[xX][0-9a-fA-F]+|0[bB][01]+|0[oO][0-7]+|[01]+[bB](?![A-Za-z0-9_])|[0-7]+[oO](?![A-Za-z0-9_])|[0-9][0-9a-fA-F]*[hH](?![A-Za-z0-9_])|(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?)/;

const ASCII_WORD = /^[A-Za-z0-9_]$/;
const UNICODE_WORD = /^[\p{L}\p{N}∞]$/u;

function isWordChar(ch: string): boolean {
	return ASCII_WORD.test(ch) || UNICODE_WORD.test(ch);
}

function isWordStart(ch: string, afterDot: boolean): boolean {
	// Unicode letters are only identifiers directly after a dot, which is where
	// a constant's display symbol (`phys.ħ`, `phys.R∞`) appears.
	return /^[A-Za-z_]$/.test(ch) || (afterDot && /^[\p{L}∞]$/u.test(ch));
}

/**
 * `FFh` is hexadecimal, but `ah` is a variable. Require either a decimal digit
 * in the prefix (a value like `1Fh`) or at least two hex characters (`FFh`);
 * that keeps single-letter names ending in h out of the number colour.
 */
function isHexSuffix(word: string): boolean {
	if (!/[hH]$/.test(word)) return false;
	const digits = word.slice(0, -1);
	if (!/^[0-9a-fA-F]+$/.test(digits)) return false;
	return /\d/.test(digits) || digits.length >= 2;
}

/** Forgetting a namespace after a non-dot token keeps `phys + config.x` honest. */
function resetContext(state: FxcTokenState): void {
	state.namespace = null;
	state.afterDot = false;
}

function classifyWord(word: string, stream: TokenStream, state: FxcTokenState): FxcToken {
	if (state.afterDot) {
		const namespace = state.namespace;
		resetContext(state);
		if (namespace === 'phys') return 'constant';
		if (namespace === 'stat') return 'variable';
		// A `#data` path (`config.offsets`) is a property, not a variable.
		return 'property';
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
	// `i` alone is a variable; only `i()` is the imaginary unit. Requiring the
	// call parenthesis for every built-in gets that right without a special case.
	if (BUILTINS.has(word) && stream.peek() === '(') return 'builtin';
	if (CONSTANTS.has(word)) return 'constant';
	if (isHexSuffix(word)) return 'number';
	if (stream.peek() === '(') return 'function';
	return 'variable';
}

/**
 * Read one token, advancing the stream. `null` means the run is whitespace and
 * should not be coloured. The function always advances (the caller in
 * `tokenizeLine` guards the zero-length case anyway).
 */
export function token(stream: TokenStream, state: FxcTokenState): FxcToken | null {
	const start = stream.pos;

	if (stream.sol()) {
		state.afterDot = false;
		state.namespace = null;
		state.afterMode = false;
	}

	if (state.inBlockComment) {
		if (!stream.skipTo('*/')) {
			stream.skipToEnd();
			return 'comment';
		}
		stream.match('*/');
		state.inBlockComment = false;
		return 'comment';
	}

	if (stream.eatSpace()) return null;

	if (stream.match('//')) {
		stream.skipToEnd();
		return 'comment';
	}
	if (stream.match('/*')) {
		if (!stream.skipTo('*/')) {
			state.inBlockComment = true;
			stream.skipToEnd();
		} else {
			stream.match('*/');
		}
		return 'comment';
	}

	if (stream.peek() === '#') {
		stream.next();
		stream.eatWhile((ch) => /[A-Za-z]/.test(ch));
		const directive = stream.string.slice(start, stream.pos);
		if (directive === '#mode') state.afterMode = true;
		return 'directive';
	}

	const quote = stream.peek();
	if (quote === '"' || quote === "'") {
		stream.next();
		while (!stream.eol()) {
			const ch = stream.peek();
			stream.next();
			if (ch === '\\') {
				if (!stream.eol()) stream.next();
				continue;
			}
			if (ch === quote) break;
		}
		resetContext(state);
		return 'string';
	}

	if (stream.match(NUMBER)) {
		resetContext(state);
		return 'number';
	}

	const first = stream.peek();
	if (first !== undefined && isWordStart(first, state.afterDot)) {
		stream.next();
		stream.eatWhile((ch) => isWordChar(ch));
		return classifyWord(stream.string.slice(start, stream.pos), stream, state);
	}

	const two = stream.string.slice(stream.pos, stream.pos + 2);
	if (LONG_OPERATORS.has(two)) {
		stream.match(two);
		resetContext(state);
		return 'operator';
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
	if (ch === 'π' || ch === '∞') return 'constant';
	if (BRACKETS.has(ch)) return 'bracket';
	if (PUNCTUATION.has(ch)) return 'punctuation';
	if (GLYPH_OPERATORS.has(ch)) return 'operator';
	if (ch === '=' || ch === '+' || ch === '-' || ch === '*' || ch === '/' || ch === '^') {
		return 'operator';
	}
	if (ch === '<' || ch === '>') return 'operator';
	return 'operator';
}

/** One run of text and what it is. Whitespace is not represented. */
export interface FxcTokenSpan {
	from: number;
	to: number;
	kind: FxcToken | null;
}

/**
 * Minimal stand-in for `StringStream`, used by `tokenize` — and by the PRGM
 * tokenizer, so the two languages share one stream implementation.
 */
export class LineStream implements TokenStream {
	pos = 0;

	constructor(readonly string: string) {}

	eol(): boolean {
		return this.pos >= this.string.length;
	}

	sol(): boolean {
		return this.pos === 0;
	}

	peek(): string | undefined {
		return this.string[this.pos];
	}

	next(): string | undefined {
		return this.pos < this.string.length ? this.string[this.pos++] : undefined;
	}

	eatWhile(match: string | RegExp | ((ch: string) => boolean)): boolean {
		const start = this.pos;
		while (!this.eol()) {
			const ch = this.peek() as string;
			if (typeof match === 'function') {
				if (!match(ch)) break;
			} else if (typeof match === 'string') {
				if (!match.includes(ch)) break;
			} else if (!match.test(ch)) {
				break;
			}
			this.pos++;
		}
		return this.pos > start;
	}

	eatSpace(): boolean {
		const start = this.pos;
		while (!this.eol() && /\s/.test(this.peek() as string)) this.pos++;
		return this.pos > start;
	}

	skipToEnd(): void {
		this.pos = this.string.length;
	}

	skipTo(ch: string): boolean {
		const found = this.string.indexOf(ch, this.pos);
		if (found < 0) return false;
		this.pos = found;
		return true;
	}

	match(
		pattern: string | RegExp,
		consume = true,
		caseInsensitive = false
	): boolean | RegExpMatchArray | null {
		if (typeof pattern === 'string') {
			const text = this.string.slice(this.pos, this.pos + pattern.length);
			const found = caseInsensitive
				? text.toLowerCase() === pattern.toLowerCase()
				: text === pattern;
			if (found && consume) this.pos += pattern.length;
			return found;
		}
		const rest = this.string.slice(this.pos);
		const found = rest.match(pattern);
		if (!found || found.index !== 0) return null;
		if (consume) this.pos += found[0].length;
		return found;
	}
}

/** Tokenize one line, carrying `state` across calls. */
export function tokenizeLine(
	line: string,
	state: FxcTokenState = createTokenState()
): { tokens: FxcTokenSpan[]; state: FxcTokenState } {
	const stream = new LineStream(line);
	const tokens: FxcTokenSpan[] = [];
	while (!stream.eol()) {
		const from = stream.pos;
		const kind = token(stream, state);
		if (stream.pos === from) {
			// A tokenizer bug must not become an infinite loop in a test.
			stream.next();
			continue;
		}
		if (kind) tokens.push({ from, to: stream.pos, kind });
	}
	return { tokens, state };
}

/** Tokenize a whole document, so a block comment can be seen crossing lines. */
export function tokenize(
	source: string,
	state: FxcTokenState = createTokenState()
): { line: number; tokens: FxcTokenSpan[] }[] {
	return source.split('\n').map((line, index) => {
		const result = tokenizeLine(line, state);
		state = result.state;
		return { line: index, tokens: result.tokens };
	});
}
