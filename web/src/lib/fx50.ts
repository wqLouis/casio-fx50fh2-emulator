/**
 * The JavaScript side of the fx-50FH II WebAssembly module.
 *
 * The module exports exactly three functions and speaks JSON, so this file is
 * the whole protocol between them and the page:
 *
 *   1. encode the request as UTF-8 and `fx_alloc` that many bytes,
 *   2. write the bytes into the module's linear memory,
 *   3. `fx_call(pointer, length)` and take the returned buffer,
 *   4. read its 4-byte little-endian length prefix and decode the rest,
 *   5. `fx_free` both buffers.
 *
 * Nothing here knows what a transpiler is. The operations live in Rust
 * (`crates/fx-wasm/src/api.rs`), which is where their behaviour is tested; the
 * types below describe what comes back over the wire and are the contract this
 * side is written against.
 */

// ---------------------------------------------------------------------------
// Positions
//
// LSP-style: zero-based line and character, so an editor can use them without
// translation. This is the one place the wasm API deliberately disagrees with
// the `fx50` command line, which prints one-based columns for people.

/** A zero-based position in a document. */
export interface Position {
	line: number;
	character: number;
}

/** A zero-based range in a document. */
export interface Range {
	start: Position;
	end: Position;
}

// ---------------------------------------------------------------------------
// The response envelope

/** Why a request failed. */
export interface FxError {
	message: string;
	/** The file the position refers to, when the request involved more than one. */
	file?: string | null;
	/** Where the problem is, when it has a position. */
	range?: Range | null;
}

/**
 * Every response says whether it worked.
 *
 * A request that cannot even be understood comes back in the same shape rather
 * than throwing, because the caller is on the other side of a WebAssembly
 * boundary and has no other way to be told.
 */
export type FxResponse<T> = ({ ok: true } & T) | { ok: false; error: FxError };

/** Narrow a response to its success case. */
export function isOk<T>(response: FxResponse<T>): response is { ok: true } & T {
	return response.ok;
}

// ---------------------------------------------------------------------------
// Requests

/**
 * The calculator's operating modes.
 *
 * The aliases are accepted by the Rust side (`Mode::parse`), which is also what
 * a `#mode` header goes through — so `#mode CPLX` and `mode: 'CPLX'` mean the
 * same thing.
 */
export type Mode =
	| 'COMP'
	| 'CMPLX'
	| 'CPLX'
	| 'COMPLEX'
	| 'BASE'
	| 'BASEN'
	| 'BASE-N'
	| 'SD'
	| 'STAT'
	| 'STATS'
	| 'STATISTICS'
	| 'REG'
	| 'REGRESSION';

/** Which source language the text is in. */
export type Language = 'fx' | 'fxc';

/**
 * The fields every operation accepts, and uses as it needs them.
 *
 * `source` may be omitted when `entry` names a file inside `files`, which is how
 * a multi-file program is built.
 */
export interface SourceOptions {
	/** The document to work on. */
	source?: string;
	/** The root of the `#include` graph. Defaults to `main.fxc`. */
	entry?: string;
	/** Replaces the filesystem: a map of path to text. */
	files?: Record<string, string>;
	/** The directory relative includes resolve against. Defaults to `entry`'s. */
	base?: string;
	/** Defaults to the language of `entry`'s extension, as the server does. */
	language?: Language;
	/** Overrides a `#mode` header. */
	mode?: Mode;
}

/** Options for `transpile`. */
export interface TranspileOptions extends SourceOptions {
	/** Emit ASCII aliases (`->`, `disp`) instead of the calculator's glyphs. */
	ascii?: boolean;
	/** Run the optional size optimisations. Defaults to true. */
	optimize?: boolean;
}

/** Options for `run`. */
export interface RunOptions extends TranspileOptions {
	/** Values for the `?` prompts, in order. */
	inputs?: (number | string)[];
}

// ---------------------------------------------------------------------------
// Responses

/** What the machine is, and the limits it imposes. */
export interface VersionInfo {
	version: string;
	modes: { name: string; description: string }[];
	limits: {
		/** Bytes of program storage, shared by all four program areas. */
		programKeys: number;
		/** The `A B C D X Y M` memories. */
		memories: number;
		constants: number;
	};
}

/** What a program costs. */
export interface SizeInfo {
	/** Bytes, one per key. */
	keys: number;
	statements: number;
	/** The cost of the largest single statement. */
	largest: number;
	capacity: number;
	fits: boolean;
	/** Bytes left, or `null` when the program does not fit. */
	remaining: number | null;
	/** The size without the optimiser, when it ran. */
	unoptimizedKeys?: number;
	/** How many bytes the optimiser saved. */
	savedKeys?: number;
}

/** How a program uses the seven memories. */
export interface MemoryPlan {
	memories: { memory: string; holders: string[] }[];
	used: number;
	free: string[];
	bindings: { name: string; memory: string }[];
	freed: string[];
	/** `const` names — inlined, so they use no memory. */
	consts: string[];
	/** `#data` table names — compile-time, so they use no memory. */
	data: string[];
}

/** One value, as the calculator would show it and as a number. */
export interface CalculatorValue {
	/** The display string, with the machine's own rounding and formatting. */
	display: string;
	/** `null` when the value is not finite, which JSON cannot carry. */
	re: number | null;
	im: number | null;
	complex: boolean;
}

/** The machine's state after a program has run. */
export interface MachineState {
	ans: CalculatorValue;
	memories: Record<string, CalculatorValue>;
	mode: string;
	angle: string;
	display: string;
	base: string | null;
}

/** The result of `transpile`. */
export interface TranspileResult {
	prgm: string;
	size: SizeInfo;
	regs?: MemoryPlan;
}

/** The result of `run`. */
export interface RunResult {
	/** The PRGM that ran, whether it was given or produced by the transpiler. */
	prgm: string;
	transpiled: boolean;
	/** The `◢` displays, in order. */
	outputs: string[];
	state: MachineState;
	size: SizeInfo;
}

/** The result of `eval`. */
export interface EvalResult {
	outputs: string[];
	state: MachineState;
}

/** One case from a `#tests` table. */
export interface TestCase {
	name: string;
	passed: boolean;
	expected: string;
	actual: string;
}

/** The result of `tests`. */
export interface TestReport {
	name: string;
	passed: number;
	failed: number;
	success: boolean;
	cases: TestCase[];
}

/** One editor marker. */
export interface Diagnostic {
	message: string;
	severity: string;
	range: Range;
	code: string | number | null;
}

/** The result of `diagnostics`. */
export interface DiagnosticsResult {
	language: string;
	diagnostics: Diagnostic[];
}

/** One completion. */
export interface CompletionItem {
	label: string;
	insertText: string;
	detail: string | null;
	/** An LSP `CompletionItemKind` name, e.g. `Function` or `Keyword`. */
	kind: string;
}

/** The result of `completions`. */
export interface CompletionsResult {
	language: string;
	items: CompletionItem[];
}

/** Documentation at a position. */
export interface HoverInfo {
	/** Markdown. */
	contents: string;
	range: Range | null;
}

/** The result of `hover`. `hover` is `null` when there is nothing there. */
export interface HoverResult {
	hover: HoverInfo | null;
}

/** One entry in the document outline. */
export interface DocumentSymbol {
	name: string;
	detail: string | null;
	kind: string;
	range: Range;
	selectionRange: Range;
	children: DocumentSymbol[] | null;
}

/** The result of `symbols`. */
export interface SymbolsResult {
	symbols: DocumentSymbol[];
}

/** One of the calculator's 40 scientific constants. */
export interface PhysicalConstant {
	/** Menu number, 1..=40. */
	code: number;
	/** The ASCII name used in source, reached as `phys.<name>`. */
	name: string;
	/** The symbol the display shows. */
	symbol: string;
	value: number | null;
	unit: string;
	description: string;
}

/** The result of `constants`. */
export interface ConstantsResult {
	constants: PhysicalConstant[];
}

// ---------------------------------------------------------------------------
// The module

/** Anything the module can be instantiated from. */
export type WasmSource = ArrayBuffer | ArrayBufferView | string | URL;

/** The raw exports, as the module provides them. */
interface Exports {
	fx_alloc(length: number): number;
	fx_free(pointer: number, length: number): void;
	fx_call(pointer: number, length: number): number;
	memory: WebAssembly.Memory;
}

/** A loaded module, with `call` bound to its exports. */
export class Fx50 {
	readonly #exports: Exports;

	constructor(exports: Exports) {
		this.#exports = exports;
	}

	/**
	 * Send one request and return the response text.
	 *
	 * `call` is what ordinary code wants; this exists for logging and for tests
	 * that want to check the module's output really is JSON.
	 */
	callRaw(request: object): string {
		const { fx_alloc, fx_free, fx_call, memory } = this.#exports;
		const encoded = new TextEncoder().encode(JSON.stringify(request));

		// `memory.buffer` is re-read at every use: allocating can grow the
		// memory, and a view taken before that would be detached.
		const input = fx_alloc(encoded.length);
		new Uint8Array(memory.buffer, input, encoded.length).set(encoded);

		const output = fx_call(input, encoded.length);
		fx_free(input, encoded.length);

		const length = new DataView(memory.buffer, output).getUint32(0, true);
		const text = new TextDecoder().decode(new Uint8Array(memory.buffer, output + 4, length));
		fx_free(output, length + 4);

		return text;
	}

	/** Send one request and return the parsed response. */
	call<T>(request: object): FxResponse<T> {
		return JSON.parse(this.callRaw(request)) as FxResponse<T>;
	}

	/** What this build is, and the limits of the machine it models. */
	version(): FxResponse<VersionInfo> {
		return this.call<VersionInfo>({ op: 'version' });
	}

	/** `.fxc` → PRGM, with what the result costs and where it puts things. */
	transpile(options: TranspileOptions): FxResponse<TranspileResult> {
		return this.call<TranspileResult>({ op: 'transpile', ...options });
	}

	/** Transpile if needed, run, and report the displays and the machine's state. */
	run(options: RunOptions): FxResponse<RunResult> {
		return this.call<RunResult>({ op: 'run', ...options });
	}

	/** Run a program's embedded `#tests` table. */
	tests(options: SourceOptions): FxResponse<TestReport> {
		return this.call<TestReport>({ op: 'tests', ...options });
	}

	/** Markers for the editor, positioned for it to use directly. */
	diagnostics(options: SourceOptions): FxResponse<DiagnosticsResult> {
		return this.call<DiagnosticsResult>({ op: 'diagnostics', ...options });
	}

	/** The completion list for a language. Does not depend on the document. */
	completions(language: Language = 'fxc'): FxResponse<CompletionsResult> {
		return this.call<CompletionsResult>({ op: 'completions', language });
	}

	/** Documentation at a zero-based position. */
	hover(options: SourceOptions & { position: Position }): FxResponse<HoverResult> {
		return this.call<HoverResult>({ op: 'hover', ...options });
	}

	/** The document outline. */
	symbols(options: SourceOptions): FxResponse<SymbolsResult> {
		return this.call<SymbolsResult>({ op: 'symbols', ...options });
	}

	/** The 40 scientific constants, for a reference panel. */
	constants(): FxResponse<ConstantsResult> {
		return this.call<ConstantsResult>({ op: 'constants' });
	}

	/** Evaluate one expression, with no program around it. */
	evaluate(options: SourceOptions): FxResponse<EvalResult> {
		return this.call<EvalResult>({ op: 'eval', ...options });
	}
}

/**
 * Instantiate the module.
 *
 * Use `WebAssembly.instantiate` and not `instantiateStreaming`: the latter needs
 * the server to send `application/wasm`, which plain static file servers often
 * do not, and the resulting failure is a confusing one. Static hosts — GitHub
 * Pages included — are exactly where that bites.
 *
 * @param input an `ArrayBuffer`, a typed array, or a URL to fetch.
 */
export async function load(input: WasmSource): Promise<Fx50> {
	const bytes =
		typeof input === 'string' || input instanceof URL
			? await (await fetch(input)).arrayBuffer()
			: input;

	// No imports: the module needs nothing from its host. `abi.rs` documents
	// why its interface is three functions and a JSON string.
	//
	// Compiled and then instantiated in two steps rather than with the
	// one-call `instantiate(bytes)`: that call is overloaded on "bytes to
	// compile" versus "a module already compiled", the two signatures are
	// ambiguous for a typed array, and the wrong one is picked. Being explicit
	// also leaves the compiled module available to instantiate again.
	const module = await WebAssembly.compile(bytes as BufferSource);
	const instance = await WebAssembly.instantiate(module, {});
	return new Fx50(instance.exports as unknown as Exports);
}

export default load;
