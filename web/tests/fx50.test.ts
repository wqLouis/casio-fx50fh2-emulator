/**
 * End-to-end tests for the WebAssembly module, run under `bun test`.
 *
 *     bun run build:wasm && bun test
 *
 * These matter more than most tests here: this is the only place the real
 * artifact — the `.wasm` file a browser downloads — is exercised. A `cargo test`
 * cannot catch a broken export, a wrong length prefix, or memory freed twice;
 * instantiating the module and calling it can. Bun has a complete
 * `WebAssembly` implementation, so no browser is needed.
 *
 * Assertions use `node:assert` rather than `expect`: they were written for the
 * plain-Node version of this file and are exact, and `bun test` reports their
 * failures perfectly well.
 */
import { describe, test } from 'bun:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { load, type FxResponse, type Hover } from '../src/lib/fx50';
import { examples, libraries } from '../src/lib/examples.generated';
import { DiagnosticSeverity } from 'vscode-languageserver-types';

const here = dirname(fileURLToPath(import.meta.url));
const bytes = await readFile(join(here, '..', 'static', 'fx_wasm.wasm'));
const fx = await load(bytes);

/** Assert a request succeeded, and narrow the type for the rest of the test. */
function expectOk<T>(response: FxResponse<T>): Extract<FxResponse<T>, { ok: true }> {
	assert.ok(response.ok, `expected success, got ${JSON.stringify(response)}`);
	return response as Extract<FxResponse<T>, { ok: true }>;
}

/** Assert a request failed, and return the message. */
function expectErr<T>(response: FxResponse<T>): string {
	assert.equal(response.ok, false, `expected failure, got ${JSON.stringify(response)}`);
	return (response as { error: { message: string } }).error.message;
}

/** A request for a single-file program, which is the common shape. */
const source = (text: string) => ({ source: text, entry: 'main.fxc' });

/**
 * The text of a hover.
 *
 * LSP lets `contents` be a `MarkupContent`, a string, or an array of either, so
 * this narrows the union rather than assuming the shape our server happens to
 * send today.
 */
function hoverText(hover: Hover | null): string {
	if (!hover) return '';
	const { contents } = hover;
	if (typeof contents === 'string') return contents;
	if (Array.isArray(contents)) {
		return contents.map((part) => (typeof part === 'string' ? part : part.value)).join('\n\n');
	}
	return contents.value;
}

describe('the module', () => {
	test('exports exactly the three ABI functions, and memory', () => {
		const module = new WebAssembly.Module(bytes);
		const names = WebAssembly.Module.exports(module)
			.map((entry) => entry.name)
			.sort();
		assert.deepEqual(names, ['fx_alloc', 'fx_call', 'fx_free', 'memory']);
	});

	test('needs nothing from its host', () => {
		// No imports at all. If this grows, every host's instantiate call has to
		// grow with it, so it is worth pinning.
		const module = new WebAssembly.Module(bytes);
		assert.deepEqual(WebAssembly.Module.imports(module), []);
	});

	test("version reports the machine's limits", () => {
		const info = expectOk(fx.version());
		assert.equal(info.limits.programKeys, 680);
		assert.equal(info.limits.memories, 7);
		assert.equal(info.limits.constants, 40);
		assert.equal(info.modes.length, 5);
	});
});

describe('transpile', () => {
	test('lowers a program and reports its size', () => {
		const result = expectOk(fx.transpile(source('fn main() { print(2 + 2); }')));
		assert.equal(result.prgm.trim(), '4◢');
		assert.equal(result.size.keys, 2);
		assert.equal(result.size.capacity, 680);
		assert.equal(result.size.fits, true);
		assert.equal(result.size.remaining, 678);
	});

	test('reports the memory plan', () => {
		const result = expectOk(
			fx.transpile(source('fn main() { let a = input(); let b = input(); print(a + b); }'))
		);
		assert.equal(result.regs?.used, 2);
		assert.equal(result.regs?.memories.length, 7);
		const holders = result.regs?.memories.flatMap((memory) => memory.holders);
		assert.deepEqual(holders, ['a', 'b']);
	});

	test('the optimiser reports what it saved', () => {
		const result = expectOk(
			fx.transpile(
				source(`fn main() {
	let a = input();
	let b = 5;
	print(a + b);
	print(b * 2);
}`)
			)
		);
		assert.equal(result.size.keys, 10);
		assert.equal(result.size.unoptimizedKeys, 14);
		assert.equal(result.size.savedKeys, 4);
		assert.ok(result.prgm.includes('A+5'), result.prgm);
	});

	test('optimize: false gives the unpropagated translation', () => {
		const result = expectOk(
			fx.transpile({
				...source('fn main() { let a = input(); let b = 5; print(a + b); }'),
				optimize: false
			})
		);
		assert.ok(result.prgm.includes('→B'), result.prgm);
	});

	test('#include is resolved from the files map, with no filesystem', () => {
		const result = expectOk(
			fx.transpile({
				entry: 'main.fxc',
				source: '#include "lib/double.fxc"\nfn main() { let a = input(); print(double(a)); }',
				files: { 'lib/double.fxc': 'fn double(x) = x * 2;' }
			})
		);
		assert.ok(result.prgm.includes('×2'), result.prgm);
		assert.ok(!result.prgm.includes('double'), result.prgm);
	});

	test('nested includes resolve relative to the including file', () => {
		const result = expectOk(
			fx.transpile({
				entry: 'main.fxc',
				source: '#include "lib/outer.fxc"\nfn main() { print(outer(1)); }',
				files: {
					'lib/outer.fxc': '#include "inner.fxc"\nfn outer(x) = inner(x) * 10;',
					'lib/inner.fxc': 'fn inner(x) = x + 1;'
				}
			})
		);
		// 1 + 1 = 2, × 10 = 20, all folded at transpile time.
		assert.equal(result.prgm.trim(), '20◢');
	});

	test('the entry document may come from the files map', () => {
		const result = expectOk(
			fx.transpile({
				entry: 'main.fxc',
				files: {
					'main.fxc': '#include "lib/one.fxc"\nfn main() { print(one()); }',
					'lib/one.fxc': 'fn one() = 1;'
				}
			})
		);
		assert.equal(result.prgm.trim(), '1◢');
	});

	test('#data is read from the files map', () => {
		const result = expectOk(
			fx.transpile({
				entry: 'main.fxc',
				source:
					'#data weights = "tables/weights.json";\nfn main() { print(weights.a + weights.b); }',
				files: { 'tables/weights.json': '{"a": 12, "b": 30}' }
			})
		);
		assert.equal(result.prgm.trim(), '42◢');
		// Compile-time values, so the program uses no memories at all.
		assert.equal(result.regs?.used, 0);
	});

	test('a missing include is an error with an LSP-style position', () => {
		const response = fx.transpile(source('#include "lib/nope.fxc"\nfn main() {}'));
		assert.equal(response.ok, false);
		if (response.ok) return;
		assert.match(response.error.message, /lib\/nope\.fxc/);
		// Zero-based, for the editor to use without translating.
		assert.equal(response.error.range?.start.line, 0);
		assert.equal(response.error.file, 'main.fxc');
	});

	test('a syntax error points at the offending text', () => {
		const response = fx.transpile(source('fn main() {\n\tprint(1 +);\n}'));
		assert.equal(response.ok, false);
		if (response.ok) return;
		// The error is on line 2 of the program as written, one-based in the
		// message and zero-based in the range.
		assert.equal(response.error.range?.start.line, 1);
	});

	test('mode is enforced: rep() needs CMPLX', () => {
		const bad = fx.transpile({
			...source('fn main() { let z = 1; print(rep(z)); }'),
			mode: 'COMP'
		});
		assert.equal(bad.ok, false);
		assert.match(expectErr(bad), /CMPLX/);

		const good = expectOk(
			fx.transpile({
				...source('fn main() { let z = 1 + 2 * i(); print(rep(z)); }'),
				mode: 'CMPLX'
			})
		);
		assert.ok(good.prgm.includes('Conjg'), good.prgm);
	});

	test('ascii selects the other output style', () => {
		const text = 'fn main() { let a = input(); print(a * 2); }';
		assert.ok(expectOk(fx.transpile(source(text))).prgm.includes('×'));
		const ascii = expectOk(fx.transpile({ ...source(text), ascii: true })).prgm;
		assert.ok(ascii.includes('*'), ascii);
		assert.ok(!ascii.includes('×'), ascii);
	});
});

describe('run', () => {
	test('returns the displays the calculator would show', () => {
		const result = expectOk(fx.run(source('fn main() { print(2 + 2); print(10 / 4); }')));
		assert.deepEqual(result.outputs, ['4', '2.5']);
	});

	test('feeds the prompts from inputs', () => {
		const result = expectOk(
			fx.run({
				...source('fn main() { let a = input(); let b = input(); print(a * b); }'),
				inputs: [6, 7]
			})
		);
		assert.deepEqual(result.outputs, ['42']);
	});

	test('reports memories and Ans as the display would show them', () => {
		const result = expectOk(
			fx.run({
				...source('fn main() { let a = input(); print(a + 1); }'),
				inputs: [41]
			})
		);
		assert.equal(result.state.memories.A.display, '41');
		assert.equal(result.state.memories.A.re, 41);
		assert.equal(result.state.ans.display, '42');
		assert.equal(result.state.mode, 'COMP');
	});

	test('rejects a non-numeric input instead of silently dropping it', () => {
		// The calculator's `?` reads a real, so an input must be a number. A
		// caller holding text parses it where it can report the failure; passing
		// a string is a contract violation and must not be quietly ignored.
		// `call` is used directly because the typed helpers refuse this at
		// compile time, and what is being tested is the wire contract.
		const response = fx.run({
			...source('fn main() { let a = input(); print(a); }'),
			inputs: [7]
		} as never);
		assert.deepEqual(expectOk(response).outputs, ['7']);

		const bad = fx.call({ op: 'run', ...source('fn main() { print(1); }'), inputs: ['7'] });
		assert.equal(bad.ok, false);
		assert.match(expectErr(bad), /invalid request/);
	});

	test('accepts a PRGM program directly', () => {
		const result = expectOk(
			fx.run({ source: '?→A:A×2◢', language: 'fx', entry: 'prog.fx', inputs: [21] })
		);
		assert.deepEqual(result.outputs, ['42']);
		assert.equal(result.transpiled, false);
	});

	test('a library with no main() says so instead of running nothing', () => {
		const message = expectErr(fx.run(source('fn double(x) = x * 2;')));
		assert.match(message, /no `fn main\(\)`/);
	});

	test('a Math ERROR is reported with a position', () => {
		const response = fx.run(source('fn main() { let a = 0; print(1 / a); }'));
		assert.equal(response.ok, false);
		if (response.ok) return;
		assert.match(response.error.message, /Math/i);
		assert.ok(response.error.range);
	});

	test('a complex result is reported both ways', () => {
		const result = expectOk(
			fx.run({
				...source('fn main() { let z = 3 + 4 * i(); print(rep(z)); print(imp(z)); }'),
				mode: 'CMPLX'
			})
		);
		assert.deepEqual(result.outputs, ['3', '4']);
		const z = result.state.memories.A;
		assert.equal(z.display, '3+4𝑖');
		assert.equal(z.re, 3);
		assert.equal(z.im, 4);
		assert.equal(z.complex, true);
	});

	test("the calculator's own rounding is what gets reported", () => {
		// 1/3 on this machine shows ten significant digits, not JavaScript's
		// seventeen. The page must show the machine's answer, not the host's.
		const result = expectOk(fx.run(source('fn main() { print(1 / 3); }')));
		assert.deepEqual(result.outputs, ['0.3333333333']);
		assert.notEqual(result.outputs[0], String(1 / 3));
	});
});

describe('#tests', () => {
	test('embedded tests are run inside the module', () => {
		const result = expectOk(
			fx.tests(
				source(`fn main() { let a = input(); print(a * 2); }
#tests = [
  { "name": "three", "input": [3], "output": ["6"] },
  { "name": "wrong", "input": [1], "output": ["99"] }
];`)
			)
		);
		assert.equal(result.passed, 1);
		assert.equal(result.failed, 1);
		assert.equal(result.success, false);
		const [good, bad] = result.cases;
		assert.equal(good.name, 'three');
		assert.equal(good.passed, true);
		assert.equal(bad.name, 'wrong');
		assert.equal(bad.expected, '99');
		assert.equal(bad.actual, '2');
	});

	test('may include a library from the files map', () => {
		// Regression: the suite parser was made loader-aware before the runner
		// was, which passed locally and failed in wasm.
		const result = expectOk(
			fx.tests({
				entry: 'main.fxc',
				source: `#include "lib/double.fxc"
fn main() { let a = input(); print(double(a)); }
#tests = [ { "name": "three", "input": [3], "output": ["6"] } ];`,
				files: { 'lib/double.fxc': 'fn double(x) = x * 2;' }
			})
		);
		assert.equal(result.passed, 1);
		assert.equal(result.failed, 0);
	});

	test("the shipped examples' own tests all pass", () => {
		// The strongest check available: every example, with its real embedded
		// `#tests`, run through the module with a file map built the way the
		// documentation pages will build it.
		assert.ok(examples.length >= 9, `only ${examples.length} examples bundled`);
		for (const example of examples) {
			const response = fx.tests({
				entry: 'main.fxc',
				files: { ...libraries, 'main.fxc': example.source }
			});
			assert.ok(response.ok, `${example.name}: ${JSON.stringify(response)}`);
			if (!response.ok) continue;
			assert.equal(
				response.failed,
				0,
				`${example.name}: ${JSON.stringify(response.cases.filter((c) => !c.passed))}`
			);
			assert.ok(response.passed > 0, `${example.name} has no tests`);
		}
	});

	test('a program with no #tests table says so', () => {
		const message = expectErr(fx.tests(source('fn main() { print(1); }')));
		assert.match(message, /no `#tests` table/);
	});
});

describe('editor features', () => {
	test('diagnostics come back as editor markers', () => {
		const result = expectOk(fx.diagnostics(source('fn main() { print(1 +); }')));
		assert.equal(result.language, 'fxc');
		assert.equal(result.diagnostics.length, 1);
		assert.equal(result.diagnostics[0].severity, DiagnosticSeverity.Error);
		assert.ok(result.diagnostics[0].range);
	});

	test('a valid program has no diagnostics', () => {
		assert.deepEqual(expectOk(fx.diagnostics(source('fn main() { print(1); }'))).diagnostics, []);
	});

	test('resolve includes through the files map', () => {
		const result = expectOk(
			fx.diagnostics({
				...source('#include "lib/one.fxc"\nfn main() { print(one()); }'),
				files: { 'lib/one.fxc': 'fn one() = 1;' }
			})
		);
		assert.deepEqual(result.diagnostics, []);
	});

	test('completions cover both languages', () => {
		const fxc = expectOk(fx.completions('fxc')).items.map((item) => item.label);
		for (const expected of ['sqrt(', 'rep(', 'phys.C0', 'stat.sumx', 'fn']) {
			assert.ok(fxc.includes(expected), `missing ${expected}`);
		}
		const prgm = expectOk(fx.completions('fx')).items.map((item) => item.label);
		assert.ok(prgm.includes('While'));
		assert.ok(!prgm.includes('sqrt('));
	});

	test('hover describes what is under the cursor', () => {
		const result = expectOk(
			fx.hover({ ...source('fn main() { print(sqrt(4)); }'), position: { line: 0, character: 18 } })
		);
		assert.match(hoverText(result.hover), /sqrt/);
		// The standard `MarkupContent`, not a flattened string.
		const contents = result.hover?.contents;
		if (
			contents &&
			typeof contents === 'object' &&
			!Array.isArray(contents) &&
			'kind' in contents
		) {
			assert.equal(contents.kind, 'markdown');
		} else {
			assert.fail(`expected markup contents, got ${JSON.stringify(contents)}`);
		}
	});

	test('hover over nothing is null, not an error', () => {
		const result = expectOk(
			fx.hover({ ...source('fn main() { }'), position: { line: 0, character: 14 } })
		);
		assert.equal(result.hover, null);
	});

	test('symbols give the document outline', () => {
		const names = expectOk(
			fx.symbols(source('fn double(x) = x * 2;\nfn main() { print(double(2)); }'))
		).symbols.map((symbol) => symbol.name);
		assert.deepEqual(names, ['double(x)', 'main()']);
	});
});

describe('constants and eval', () => {
	test('lists all forty', () => {
		const constants = expectOk(fx.constants()).constants;
		assert.equal(constants.length, 40);
		assert.equal(constants[0].code, 1);
		assert.equal(constants[39].code, 40);
		const c0 = constants.find((constant) => constant.name === 'C0');
		assert.equal(c0?.value, 299792458);
	});

	test('eval works on an expression with no program', () => {
		const result = expectOk(fx.evaluate({ source: '2 + 3 × 4' }));
		assert.deepEqual(result.outputs, ['14']);
		assert.equal(result.state.ans.display, '14');
	});

	test('eval honours a forced mode', () => {
		const result = expectOk(fx.evaluate({ source: '(3 + 4i) × (1 - 2i)', mode: 'CMPLX' }));
		assert.deepEqual(result.outputs, ['11-2𝑖']);
	});
});

describe('robustness', () => {
	test('every error path is still valid JSON, with no NaN or Infinity', () => {
		const requests = [
			{ op: 'version' },
			{ op: 'nope' },
			{ op: 'transpile' },
			{ op: 'transpile', ...source('fn main() { print(1); }') },
			{ op: 'transpile', ...source('fn main() { print(1 +); }') },
			{ op: 'run', ...source('fn main() { let a = 0; print(1 / a); }') },
			{ op: 'eval', source: '0 / 0' },
			{ op: 'hover', ...source('fn main() {}') }
		];
		for (const request of requests) {
			const text = fx.callRaw(request);
			assert.ok(!text.includes('NaN'), text);
			assert.ok(!text.includes('Infinity'), text);
			// Throws if it is not JSON, which is the assertion.
			JSON.parse(text);
		}
	});

	test('a very large program does not corrupt memory', () => {
		// Exercises the allocator and the length prefix on a response much
		// larger than the initial memory page.
		const lines = Array.from({ length: 400 }, (_, i) => `\tprint(${i} + 1);`);
		const result = expectOk(fx.run(source(`fn main() {\n${lines.join('\n')}\n}`)));
		assert.equal(result.outputs.length, 400);
		assert.equal(result.outputs[399], '400');
	});

	test('many sequential calls do not leak or detach memory', () => {
		// Each call allocates and frees; a mistake in the pointer dance usually
		// shows up as a trap or a corrupted answer under repetition.
		for (let i = 0; i < 200; i += 1) {
			assert.equal(expectOk(fx.evaluate({ source: `${i} + 1` })).outputs[0], String(i + 1));
		}
	});

	test('a request containing non-ASCII text survives the round trip', () => {
		// The calculator's glyphs are multi-byte, so this is the common case:
		// `.fxc` goes in, PRGM glyphs come back out.
		const result = expectOk(
			fx.transpile(source('fn main() { let a = input(); print(sqrt(a) + pi); }'))
		);
		assert.ok(result.prgm.includes('√'), result.prgm);
		assert.ok(result.prgm.includes('π'), result.prgm);
		// And a PRGM program written with those glyphs runs.
		expectOk(fx.run({ source: '√(16)+π◢', language: 'fx', entry: 'prog.fx' }));
	});
});
