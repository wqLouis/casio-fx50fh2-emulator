// End-to-end tests for the WebAssembly module, run under Node.
//
//     ./web/build.sh && node web/test.mjs
//
// These matter more than most tests in the repository: they are the only place
// the real artifact — the `.wasm` file a browser downloads — is exercised. A
// `cargo test` cannot catch a broken export, a wrong length prefix, or memory
// that was freed twice; instantiating the module and calling it can. Node has a
// complete `WebAssembly` implementation, so no browser is needed.

import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import assert from "node:assert/strict";

import { load } from "./fx50.js";

const here = dirname(fileURLToPath(import.meta.url));

let passed = 0;
let failed = 0;

function test(name, body) {
  try {
    body();
    passed += 1;
    console.log(`  ok    ${name}`);
  } catch (error) {
    failed += 1;
    console.log(`  FAIL  ${name}`);
    console.log(`        ${error.message.split("\n").join("\n        ")}`);
  }
}

const bytes = await readFile(join(here, "fx_wasm.wasm"));
const fx = await load(bytes);

console.log(`fx-wasm ${fx.version().version} (${bytes.length} bytes)\n`);

// ---------------------------------------------------------------------------
// The module itself

test("the module exports exactly the three ABI functions", () => {
  const module = new WebAssembly.Module(bytes);
  const names = WebAssembly.Module.exports(module)
    .map((entry) => entry.name)
    .sort();
  assert.deepEqual(names, ["fx_alloc", "fx_call", "fx_free", "memory"]);
});

test("the module needs nothing from its host", () => {
  // No imports at all: if this ever grows, the page's instantiate call has to
  // grow with it, so it is worth pinning.
  const module = new WebAssembly.Module(bytes);
  assert.deepEqual(WebAssembly.Module.imports(module), []);
});

test("version reports the machine's limits", () => {
  const response = fx.version();
  assert.equal(response.ok, true);
  assert.equal(response.limits.programKeys, 680);
  assert.equal(response.limits.memories, 7);
  assert.equal(response.limits.constants, 40);
  assert.equal(response.modes.length, 5);
});

// ---------------------------------------------------------------------------
// transpile

test("transpile lowers a program and reports its size", () => {
  const response = fx.transpile("fn main() { print(2 + 2); }");
  assert.equal(response.ok, true);
  assert.equal(response.prgm.trim(), "4◢");
  assert.equal(response.size.keys, 2);
  assert.equal(response.size.capacity, 680);
  assert.equal(response.size.fits, true);
  assert.equal(response.size.remaining, 678);
});

test("transpile reports the memory plan", () => {
  const response = fx.transpile(
    "fn main() { let a = input(); let b = input(); print(a + b); }",
  );
  assert.equal(response.regs.used, 2);
  assert.equal(response.regs.memories.length, 7);
  const holders = response.regs.memories.flatMap((memory) => memory.holders);
  assert.deepEqual(holders, ["a", "b"]);
});

test("the optimiser reports what it saved", () => {
  const source = `fn main() {
    let a = input();
    let b = 5;
    print(a + b);
    print(b * 2);
}`;
  const response = fx.transpile(source);
  assert.equal(response.size.keys, 10);
  assert.equal(response.size.unoptimizedKeys, 14);
  assert.equal(response.size.savedKeys, 4);
  assert.ok(response.prgm.includes("A+5"), response.prgm);
});

test("optimize: false gives the unpropagated translation", () => {
  const source = "fn main() { let a = input(); let b = 5; print(a + b); }";
  const response = fx.transpile(source, { optimize: false });
  assert.equal(response.ok, true);
  assert.ok(response.prgm.includes("→B"), response.prgm);
});

test("#include is resolved from the files map, with no filesystem", () => {
  const response = fx.transpile(
    '#include "lib/double.fxc"\nfn main() { let a = input(); print(double(a)); }',
    {
      entry: "main.fxc",
      files: { "lib/double.fxc": "fn double(x) = x * 2;" },
    },
  );
  assert.equal(response.ok, true, JSON.stringify(response.error));
  assert.ok(response.prgm.includes("×2"), response.prgm);
  assert.ok(!response.prgm.includes("double"), response.prgm);
});

test("nested includes resolve relative to the including file", () => {
  const response = fx.transpile(
    '#include "lib/outer.fxc"\nfn main() { print(outer(1)); }',
    {
      entry: "main.fxc",
      files: {
        "lib/outer.fxc": '#include "inner.fxc"\nfn outer(x) = inner(x) * 10;',
        "lib/inner.fxc": "fn inner(x) = x + 1;",
      },
    },
  );
  assert.equal(response.ok, true, JSON.stringify(response.error));
  // 1 + 1 = 2, × 10 = 20, all folded at transpile time.
  assert.equal(response.prgm.trim(), "20◢");
});

test("the entry document may come from the files map", () => {
  const response = fx.call({
    op: "transpile",
    entry: "main.fxc",
    files: {
      "main.fxc": '#include "lib/one.fxc"\nfn main() { print(one()); }',
      "lib/one.fxc": "fn one() = 1;",
    },
  });
  assert.equal(response.ok, true, JSON.stringify(response.error));
  assert.equal(response.prgm.trim(), "1◢");
});

test("#data is read from the files map", () => {
  const response = fx.transpile(
    '#data weights = "tables/weights.json";\nfn main() { print(weights.a + weights.b); }',
    { entry: "main.fxc", files: { "tables/weights.json": '{"a": 12, "b": 30}' } },
  );
  assert.equal(response.ok, true, JSON.stringify(response.error));
  assert.equal(response.prgm.trim(), "42◢");
  // The values are compile-time, so the program uses no memories at all.
  assert.equal(response.regs.used, 0);
});

test("a missing include is an error with an LSP-style position", () => {
  const response = fx.transpile('#include "lib/nope.fxc"\nfn main() {}');
  assert.equal(response.ok, false);
  assert.match(response.error.message, /lib\/nope\.fxc/);
  // 0-based, for the editor to use without translating.
  assert.equal(response.error.range.start.line, 0);
  assert.equal(response.error.file, "main.fxc");
});

test("a syntax error points at the offending text", () => {
  const response = fx.transpile("fn main() {\n    print(1 +);\n}");
  assert.equal(response.ok, false);
  assert.ok(response.error.range.start.line >= 0);
});

test("mode is enforced: rep() needs CMPLX", () => {
  const bad = fx.transpile("fn main() { let z = 1; print(rep(z)); }", {
    mode: "COMP",
  });
  assert.equal(bad.ok, false);
  assert.match(bad.error.message, /CMPLX/);

  const good = fx.transpile("fn main() { let z = 1 + 2 * i(); print(rep(z)); }", {
    mode: "CMPLX",
  });
  assert.equal(good.ok, true, JSON.stringify(good.error));
  assert.ok(good.prgm.includes("Conjg"), good.prgm);
});

test("ascii selects the other output style", () => {
  const source = "fn main() { let a = input(); print(a * 2); }";
  assert.ok(fx.transpile(source).prgm.includes("×"));
  const ascii = fx.transpile(source, { ascii: true }).prgm;
  assert.ok(ascii.includes("*"), ascii);
  assert.ok(!ascii.includes("×"), ascii);
});

// ---------------------------------------------------------------------------
// run

test("run returns the displays the calculator would show", () => {
  const response = fx.run("fn main() { print(2 + 2); print(10 / 4); }");
  assert.equal(response.ok, true, JSON.stringify(response.error));
  assert.deepEqual(response.outputs, ["4", "2.5"]);
});

test("run feeds the prompts from inputs", () => {
  const response = fx.run(
    "fn main() { let a = input(); let b = input(); print(a * b); }",
    { inputs: [6, 7] },
  );
  assert.deepEqual(response.outputs, ["42"]);
});

test("run reports memories and Ans as the display would show them", () => {
  const response = fx.run("fn main() { let a = input(); print(a + 1); }", {
    inputs: [41],
  });
  assert.equal(response.state.memories.A.display, "41");
  assert.equal(response.state.memories.A.re, 41);
  assert.equal(response.state.ans.display, "42");
  assert.equal(response.state.mode, "COMP");
});

test("non-numeric inputs are ignored rather than becoming NaN", () => {
  const response = fx.run("fn main() { let a = input(); print(a); }", {
    inputs: ["7", "not a number"],
  });
  assert.deepEqual(response.outputs, ["7"]);
});

test("run accepts a PRGM program directly", () => {
  const response = fx.run("?→A:A×2◢", {
    language: "fx",
    entry: "prog.fx",
    inputs: [21],
  });
  assert.equal(response.ok, true, JSON.stringify(response.error));
  assert.deepEqual(response.outputs, ["42"]);
  assert.equal(response.transpiled, false);
});

test("a library with no main() says so instead of running nothing", () => {
  const response = fx.run("fn double(x) = x * 2;", { entry: "lib/double.fxc" });
  assert.equal(response.ok, false);
  assert.match(response.error.message, /no `fn main\(\)`/);
});

test("a Math ERROR is reported with a position", () => {
  const response = fx.run("fn main() { let a = 0; print(1 / a); }");
  assert.equal(response.ok, false);
  assert.match(response.error.message, /Math/i);
  assert.ok(response.error.range);
});

test("a complex result is reported both ways", () => {
  const response = fx.run(
    "fn main() { let z = 3 + 4 * i(); print(rep(z)); print(imp(z)); }",
    { mode: "CMPLX" },
  );
  assert.deepEqual(response.outputs, ["3", "4"]);
  const z = response.state.memories.A;
  assert.equal(z.display, "3+4𝑖");
  assert.equal(z.re, 3);
  assert.equal(z.im, 4);
  assert.equal(z.complex, true);
});

test("the calculator's own rounding is what gets reported", () => {
  // 1/3 on this machine shows ten significant digits, not JavaScript's
  // seventeen. The page must show the machine's answer, not the host's.
  const response = fx.run("fn main() { print(1 / 3); }");
  assert.deepEqual(response.outputs, ["0.3333333333"]);
  assert.notEqual(response.outputs[0], String(1 / 3));
});

// ---------------------------------------------------------------------------
// tests

test("embedded #tests are run inside the module", () => {
  const response = fx.tests(`fn main() { let a = input(); print(a * 2); }
#tests = [
  { "name": "three", "input": [3], "output": ["6"] },
  { "name": "wrong", "input": [1], "output": ["99"] }
];`);
  assert.equal(response.ok, true, JSON.stringify(response.error));
  assert.equal(response.passed, 1);
  assert.equal(response.failed, 1);
  assert.equal(response.success, false);
  const [good, bad] = response.cases;
  assert.equal(good.name, "three");
  assert.equal(good.passed, true);
  assert.equal(bad.name, "wrong");
  assert.equal(bad.expected, "99");
  assert.equal(bad.actual, "2");
});

test("embedded tests may include a library from the files map", () => {
  // Regression: the suite parser was made loader-aware before the runner was,
  // which passed locally and failed in wasm.
  const response = fx.call({
    op: "tests",
    entry: "main.fxc",
    source: `#include "lib/double.fxc"
fn main() { let a = input(); print(double(a)); }
#tests = [ { "name": "three", "input": [3], "output": ["6"] } ];`,
    files: { "lib/double.fxc": "fn double(x) = x * 2;" },
  });
  assert.equal(response.ok, true, JSON.stringify(response.error));
  assert.equal(response.passed, 1);
  assert.equal(response.failed, 0);
});

test("the shipped examples' own tests all pass", async () => {
  // The strongest check available: every example the site offers, with its real
  // embedded #tests, run through the wasm module with the same file map the
  // page builds.
  const { examples, libraries } = await import("./examples.js");
  assert.ok(examples.length >= 9, `only ${examples.length} examples bundled`);
  for (const example of examples) {
    const response = fx.call({
      op: "tests",
      entry: "main.fxc",
      files: { ...libraries, "main.fxc": example.source },
    });
    assert.equal(
      response.ok,
      true,
      `${example.name}: ${JSON.stringify(response.error)}`,
    );
    assert.equal(response.failed, 0, `${example.name}: ${JSON.stringify(response.cases.filter((c) => !c.passed))}`);
    assert.ok(response.passed > 0, `${example.name} has no tests`);
  }
});

test("a program with no #tests table says so", () => {
  const response = fx.tests("fn main() { print(1); }");
  assert.equal(response.ok, false);
  assert.match(response.error.message, /no `#tests` table/);
});

// ---------------------------------------------------------------------------
// editor features

test("diagnostics come back as editor markers", () => {
  const response = fx.diagnostics("fn main() { print(1 +); }");
  assert.equal(response.ok, true);
  assert.equal(response.language, "fxc");
  assert.equal(response.diagnostics.length, 1);
  assert.equal(response.diagnostics[0].severity, "error");
  assert.ok(response.diagnostics[0].range);
});

test("a valid program has no diagnostics", () => {
  assert.deepEqual(fx.diagnostics("fn main() { print(1); }").diagnostics, []);
});

test("diagnostics resolve includes through the files map", () => {
  const response = fx.diagnostics(
    '#include "lib/one.fxc"\nfn main() { print(one()); }',
    { files: { "lib/one.fxc": "fn one() = 1;" } },
  );
  assert.deepEqual(response.diagnostics, []);
});

test("completions cover both languages", () => {
  const fxc = fx.completions("fxc").items.map((item) => item.label);
  for (const expected of ["sqrt(", "rep(", "phys.C0", "stat.sumx", "fn"]) {
    assert.ok(fxc.includes(expected), `missing ${expected}`);
  }
  const fxItems = fx.completions("fx").items.map((item) => item.label);
  assert.ok(fxItems.includes("While"));
  assert.ok(!fxItems.includes("sqrt("));
});

test("hover describes what is under the cursor", () => {
  const response = fx.hover("fn main() { print(sqrt(4)); }", 0, 18);
  assert.equal(response.ok, true);
  assert.match(response.hover.contents, /sqrt/);
});

test("hover over nothing is null, not an error", () => {
  const response = fx.hover("fn main() { }", 0, 14);
  assert.equal(response.ok, true);
  assert.equal(response.hover, null);
});

test("symbols give the document outline", () => {
  const names = fx
    .symbols("fn double(x) = x * 2;\nfn main() { print(double(2)); }")
    .symbols.map((symbol) => symbol.name);
  assert.deepEqual(names, ["double(x)", "main()"]);
});

// ---------------------------------------------------------------------------
// constants and eval

test("constants lists all forty", () => {
  const constants = fx.constants().constants;
  assert.equal(constants.length, 40);
  assert.equal(constants[0].code, 1);
  assert.equal(constants[39].code, 40);
  const c0 = constants.find((constant) => constant.name === "C0");
  assert.equal(c0.value, 299792458);
});

test("eval works on an expression with no program", () => {
  const response = fx.evaluate("2 + 3 × 4");
  assert.deepEqual(response.outputs, ["14"]);
  assert.equal(response.state.ans.display, "14");
});

test("eval honours a forced mode", () => {
  const response = fx.evaluate("(3 + 4i) × (1 - 2i)", { mode: "CMPLX" });
  assert.deepEqual(response.outputs, ["11-2𝑖"]);
});

// ---------------------------------------------------------------------------
// Robustness: a page calls JSON.parse on whatever comes back

test("every error path is still valid JSON, with no NaN or Infinity", () => {
  const requests = [
    { op: "version" },
    { op: "nope" },
    { op: "transpile" },
    { op: "transpile", source: "fn main() { print(1); }" },
    { op: "transpile", source: "fn main() { print(1 +); }" },
    { op: "run", source: "fn main() { let a = 0; print(1 / a); }" },
    { op: "eval", source: "0 / 0" },
    { op: "hover", source: "fn main() {}" },
  ];
  for (const request of requests) {
    const text = fx.callRaw(request);
    assert.ok(!text.includes("NaN"), text);
    assert.ok(!text.includes("Infinity"), text);
    // Throws if it is not JSON, which is the assertion.
    JSON.parse(text);
  }
});

test("a very large program does not corrupt memory", () => {
  // Exercises the allocator and the length prefix on a response much larger
  // than the initial memory page.
  const lines = [];
  for (let i = 0; i < 400; i += 1) lines.push(`    print(${i} + 1);`);
  const source = `fn main() {\n${lines.join("\n")}\n}`;
  const response = fx.run(source);
  assert.equal(response.ok, true, JSON.stringify(response.error));
  assert.equal(response.outputs.length, 400);
  assert.equal(response.outputs[399], "400");
});

test("many sequential calls do not leak or detach memory", () => {
  // Each call allocates and frees; a mistake in the pointer dance usually shows
  // up as a trap or a corrupted answer under repetition.
  for (let i = 0; i < 200; i += 1) {
    const response = fx.evaluate(`${i} + 1`);
    assert.equal(response.outputs[0], String(i + 1));
  }
});

test("a request containing non-ASCII text survives the round trip", () => {
  // The calculator's glyphs are multi-byte, so this is the common case: `.fxc`
  // goes in, PRGM glyphs come back out. `a` is a prompt rather than a constant
  // so the glyphs are not folded away.
  const response = fx.transpile(
    "fn main() { let a = input(); print(sqrt(a) + pi); }",
  );
  assert.equal(response.ok, true, JSON.stringify(response.error));
  assert.ok(response.prgm.includes("√"), response.prgm);
  assert.ok(response.prgm.includes("π"), response.prgm);
  // And a PRGM program written with those glyphs runs.
  const ran = fx.run("√(16)+π◢", { language: "fx", entry: "prog.fx" });
  assert.equal(ran.ok, true, JSON.stringify(ran.error));
});

console.log(`\n${passed} passed, ${failed} failed`);
process.exit(failed === 0 ? 0 : 1);
