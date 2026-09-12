/*
 * The fx-50FH II workbench page.
 *
 * Everything real happens in Rust, compiled to WebAssembly: this file only moves
 * text between the editor and `fx_wasm.wasm` and draws the answers. There is no
 * transpiler or interpreter logic here, and no second implementation to drift.
 *
 * The module speaks JSON (`web/fx50.js` wraps the ABI), so the wiring is:
 *
 *   editor change -> {op:"diagnostics"}  -> markers
 *                 -> {op:"transpile"}    -> the PRGM listing and its size
 *   Run button    -> {op:"run"}          -> the displays and the memories
 *
 * Monaco is loaded from a CDN. If it cannot be fetched — an offline machine, a
 * blocked CDN — the page falls back to a plain textarea so that transpiling and
 * running still work, and says so rather than appearing broken.
 */

import { load } from "./fx50.js";
import { examples, libraries } from "./examples.js";

const MONACO_VERSION = "0.52.2";
const MONACO_BASE = `https://cdn.jsdelivr.net/npm/monaco-editor@${MONACO_VERSION}/min/vs`;
const ENTRY = "main.fxc";

/** Maps a diagnostic severity from the module onto Monaco's marker severities. */
const MARKER_SEVERITY = { error: 8, warning: 4, information: 2, hint: 1 };

/** Maps an LSP completion kind name onto Monaco's numeric kind. */
const COMPLETION_KIND = {
  Text: 18,
  Method: 0,
  Function: 1,
  Constructor: 2,
  Field: 3,
  Variable: 4,
  Class: 5,
  Interface: 7,
  Module: 8,
  Property: 9,
  Unit: 12,
  Value: 13,
  Enum: 15,
  Keyword: 17,
  Snippet: 27,
  Color: 19,
  File: 20,
  Reference: 21,
  Folder: 23,
  Constant: 14,
  Struct: 6,
  Event: 10,
  Operator: 11,
  TypeParameter: 24,
};

// ---------------------------------------------------------------------------
// Page state

/** Every file the page is holding, keyed by path. Always contains `main.fxc`. */
const files = { [ENTRY]: "", ...libraries };

/** Which file the editor is showing. */
let current = ENTRY;

let editor = null;
let fallback = null;
let fx = null;

const $ = (id) => document.getElementById(id);

// ---------------------------------------------------------------------------
// Boot

async function main() {
  try {
    fx = await load("./fx_wasm.wasm");
  } catch (error) {
    $("status").textContent = `could not load the wasm module: ${error.message}`;
    return;
  }

  reportVersion();
  await startEditor();
  loadExample(examples[0]?.name);
  renderConstantList();
}

function reportVersion() {
  const info = fx.version();
  $("version").textContent = `fx50 ${info.version}`;
  $("limits").textContent =
    `${info.limits.programKeys} bytes of program storage, ` +
    `${info.limits.memories} memories, ${info.limits.constants} constants`;
}

// ---------------------------------------------------------------------------
// Editor

/** Monaco has no built-in `fxc`, so the language is registered here. */
function registerFxc(monaco) {
  monaco.languages.register({ id: "fxc", extensions: [".fxc"] });

  monaco.languages.setMonarchTokensProvider("fxc", {
    // The calculator's own glyphs are ordinary operators in `.fxc` source too.
    operators: [
      "+", "-", "*", "/", "^", "%", "=", "<", ">", "<=", ">=", "==", "!=",
      "&&", "||", "!", "->", "=>", "×", "÷", "≠", "≤", "≥", "∠", "π", "√",
    ],
    keywords: [
      "fn", "let", "return", "print", "if", "else", "while", "for", "in",
      "break", "continue", "const", "free", "unsafe_free", "true", "false",
      "and", "or", "not", "then", "else_if",
    ],
    builtins: [
      "sin", "cos", "tan", "asin", "acos", "atan", "sinh", "cosh", "tanh",
      "asinh", "acosh", "atanh", "log", "ln", "sqrt", "cbrt", "root", "abs",
      "rnd", "inv", "sqr", "cube", "fact", "pct", "frac", "npr", "ncr", "pol",
      "rec", "arg", "conjg", "rep", "imp", "polar", "to_cartesian", "to_polar",
      "dms", "neg", "ran", "ans", "mvalue", "i", "input", "mplus", "mminus",
      "clrmemory", "clrstat", "freqon", "freqoff", "dt", "deg", "rad", "gra",
      "fix", "sci", "norm", "dec", "hex", "bin", "oct", "re_im", "stat",
      "phys",
    ],

    tokenizer: {
      root: [
        // Directives are preprocessor lines, so they are matched first.
        [/^\s*#(mode|include|data|tests)\b/, "directive"],
        [/#[A-Za-z_]\w*/, "directive.invalid"],

        [/\/\/.*$/, "comment"],
        [/\/\*/, "comment", "@blockComment"],

        [/"/, "string", "@string"],

        // `phys.c` and `stat.sumx` are namespaced names.
        [/\b(phys|stat)\.[A-Za-z_]\w*/, "namespace"],

        [/\d+(\.\d+)?([eE][-+]?\d+)?/, "number"],
        [/0[xX][0-9a-fA-F]+/, "number.hex"],
        [/\d+[Ff]h\b/, "number.hex"],
        [/\d+[Bb]\b/, "number.binary"],

        [/[A-Za-z_]\w*(?=\s*\()/, {
          cases: {
            "@builtins": "predefined",
            "@keywords": "keyword",
            "@default": "identifier",
          },
        }],
        [/[A-Za-z_]\w*/, {
          cases: { "@keywords": "keyword", "@default": "identifier" },
        }],

        [/[{}()\[\]]/, "@brackets"],
        // Two-character operators must be tried before the single-character rule
        // below, or `<=` would tokenize as `<` followed by `=`.
        [/[=!<>]=|!=|==|<>|&&|\|\||->|=>/, "operator"],
        [/[-+*/%^<>]/, "operator"],
        [/[×÷≠≤≥∠π√]/, "operator"],
        [/[;:,.]/, "delimiter"],
      ],
      blockComment: [
        [/[^/*]+/, "comment"],
        [/\*\//, "comment", "@pop"],
        [/[/*]/, "comment"],
      ],
      string: [
        [/[^\\"]+/, "string"],
        [/\\./, "string.escape"],
        [/"/, "string", "@pop"],
      ],
    },
  });

  monaco.languages.setLanguageConfiguration("fxc", {
    comments: { lineComment: "//", blockComment: ["/*", "*/"] },
    brackets: [["{", "}"], ["(", ")"], ["[", "]"]],
    autoClosingPairs: [
      { open: "{", close: "}" },
      { open: "(", close: ")" },
      { open: "[", close: "]" },
      { open: '"', close: '"' },
    ],
    surroundingPairs: [
      { open: "{", close: "}" },
      { open: "(", close: ")" },
      { open: "[", close: "]" },
      { open: '"', close: '"' },
    ],
  });

  // Diagnostics, completions and hover all come straight from the wasm module,
  // which is running the same code the editor extensions do.
  monaco.languages.registerCompletionItemProvider("fxc", {
    provideCompletionItems() {
      const response = fx.completions("fxc");
      return {
        suggestions: response.items.map((item) => ({
          label: item.label,
          insertText: item.insertText,
          detail: item.detail ?? undefined,
          kind: COMPLETION_KIND[item.kind] ?? 18,
          // The snippet form uses `${1:…}` placeholders, which is Monaco's
          // InsertTextFormat.Snippet.
          insertTextRules: item.insertText.includes("$")
            ? monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet
            : undefined,
        })),
      };
    },
  });

  monaco.languages.registerHoverProvider("fxc", {
    provideHover(model, position) {
      const response = fx.call({
        op: "hover",
        source: model.getValue(),
        language: "fxc",
        entry: current,
        position: { line: position.lineNumber - 1, character: position.column - 1 },
      });
      if (!response.ok || !response.hover) return null;
      const range = response.hover.range
        ? new monaco.Range(
            response.hover.range.start.line + 1,
            response.hover.range.start.character + 1,
            response.hover.range.end.line + 1,
            response.hover.range.end.character + 1,
          )
        : undefined;
      return { contents: [{ value: response.hover.contents }], range };
    },
  });
}

async function startEditor() {
  try {
    await loadMonaco();
    registerFxc(window.monaco);
    editor = window.monaco.editor.create($("editor"), {
      value: "",
      language: "fxc",
      theme: "vs-dark",
      automaticLayout: true,
      minimap: { enabled: false },
      fontSize: 14,
      tabSize: 4,
      scrollBeyondLastLine: false,
      renderWhitespace: "selection",
    });
    editor.onDidChangeModelContent(scheduleTranspile);
    $("editor-fallback").hidden = true;
  } catch (error) {
    useFallbackEditor(error);
  }
}

/** Load the Monaco AMD bundle, or reject if it does not arrive. */
function loadMonaco() {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(
      () => reject(new Error("timed out fetching Monaco from the CDN")),
      15000,
    );
    const script = document.createElement("script");
    script.src = `${MONACO_BASE}/loader.js`;
    script.onerror = () => {
      clearTimeout(timer);
      reject(new Error("could not fetch Monaco from the CDN"));
    };
    script.onload = () => {
      window.require.config({ paths: { vs: MONACO_BASE } });
      window.require(["vs/editor/editor.main"], () => {
        clearTimeout(timer);
        resolve();
      });
    };
    document.head.appendChild(script);
  });
}

/**
 * A plain textarea, used when Monaco is unavailable.
 *
 * Everything except the editor decorations still works, which is the point: a
 * page that can transpile and run is useful even with no CDN.
 */
function useFallbackEditor(error) {
  fallback = document.createElement("textarea");
  fallback.id = "fallback-textarea";
  fallback.spellcheck = false;
  fallback.addEventListener("input", () => {
    files[current] = fallback.value;
    scheduleTranspile();
  });
  $("editor").replaceChildren(fallback);
  $("editor-note").textContent =
    `Editor features are off (${error.message}); transpiling and running still work.`;
}

/** The editor's text, from whichever editor is active. */
function currentText() {
  if (editor) return editor.getValue();
  if (fallback) return fallback.value;
  return files[current] ?? "";
}

function setEditorText(text) {
  if (editor) {
    editor.setValue(text);
  } else if (fallback) {
    fallback.value = text;
  }
  files[current] = text;
  scheduleTranspile();
}

// ---------------------------------------------------------------------------
// Transpiling

let pending = null;

/** Re-transpile shortly after typing stops, so a keystroke does not block. */
function scheduleTranspile() {
  files[current] = currentText();
  if (pending) clearTimeout(pending);
  pending = setTimeout(() => {
    pending = null;
    refresh();
  }, 180);
}

/** Publish diagnostics for the file being edited. */
function publishDiagnostics(source) {
  const response = fx.call({
    op: "diagnostics",
    source,
    language: "fxc",
    entry: current,
    files,
  });
  const markers = (response.diagnostics ?? []).map((diagnostic) => ({
    severity: MARKER_SEVERITY[diagnostic.severity] ?? 8,
    message: diagnostic.message,
    startLineNumber: diagnostic.range.start.line + 1,
    startColumn: diagnostic.range.start.character + 1,
    endLineNumber: diagnostic.range.end.line + 1,
    endColumn: diagnostic.range.end.character + 1,
    code: diagnostic.code ?? undefined,
  }));

  if (editor && window.monaco) {
    window.monaco.editor.setModelMarkers(editor.getModel(), "fx50", markers);
  }

  $("diagnostics").innerHTML = "";
  for (const marker of markers) {
    const row = document.createElement("div");
    row.className = "diagnostic";
    row.textContent = `${marker.startLineNumber}:${marker.startColumn}  ${marker.message}`;
    $("diagnostics").appendChild(row);
  }
  $("diagnostics-count").textContent = markers.length
    ? `${markers.length} problem(s)`
    : "no problems";
}

/** Transpile `main.fxc` and draw the listing, size and memory plan. */
function refresh() {
  const source = files[ENTRY];

  // Diagnostics are published for the file being edited, which is not
  // necessarily the entry document — a library is checked on its own.
  publishDiagnostics(currentText());

  const response = fx.call({ op: "transpile", entry: ENTRY, files });

  if (!response.ok) {
    $("prgm").textContent = "";
    $("transpile-error").textContent = response.error.message;
    $("transpile-error").hidden = false;
    $("size-bar").style.width = "0%";
    $("size-text").textContent = "\u2014";
    $("memory").innerHTML = "";
    return;
  }

  $("transpile-error").hidden = true;
  $("prgm").textContent = response.prgm;
  drawSize(response.size);
  drawMemory(response.regs);
}

function drawSize(size) {
  const percent = Math.min(100, (100 * size.keys) / size.capacity);
  const bar = $("size-bar");
  bar.style.width = `${percent}%`;
  bar.className = size.fits ? "bar-fill" : "bar-fill over";

  const saved = size.savedKeys
    ? ` \u00b7 ${size.savedKeys} saved by the optimiser (${size.unoptimizedKeys} without it)`
    : "";
  $("size-text").textContent = size.fits
    ? `${size.keys} of ${size.capacity} bytes \u00b7 ${size.remaining} left${saved}`
    : `${size.keys} of ${size.capacity} bytes \u00b7 ${size.keys - size.capacity} over${saved}`;
}

function drawMemory(regs) {
  const host = $("memory");
  host.innerHTML = "";
  if (!regs) return;

  for (const memory of regs.memories) {
    const row = document.createElement("div");
    row.className = memory.holders.length ? "memory used" : "memory";
    const holders = memory.holders.length
      ? memory.holders.join(" \u2192 ")
      : "\u2014";
    row.innerHTML =
      `<span class="memory-name">${memory.memory}</span>` +
      `<span class="memory-holders">${escapeHtml(holders)}</span>`;
    host.appendChild(row);
  }

  const note = document.createElement("div");
  note.className = "memory-note";
  const compiletime = [];
  if (regs.consts.length) compiletime.push(`${regs.consts.length} \`const\``);
  if (regs.data.length) compiletime.push(`${regs.data.length} \`#data\` table(s)`);
  note.textContent =
    `${regs.used} of 7 memories used \u00b7 free: ${regs.free.join(" ") || "none"}` +
    (compiletime.length ? ` \u00b7 no memory: ${compiletime.join(", ")}` : "");
  host.appendChild(note);
}

// ---------------------------------------------------------------------------
// Running

function run() {
  const source = files[ENTRY];
  const inputs = parseInputs($("inputs").value);
  const response = fx.call({ op: "run", entry: ENTRY, files, inputs, source });

  const output = $("run-output");
  output.innerHTML = "";

  if (!response.ok) {
    const row = document.createElement("div");
    row.className = "run-error";
    row.textContent = response.error.message;
    output.appendChild(row);
    return;
  }

  if (response.outputs.length === 0) {
    const row = document.createElement("div");
    row.className = "run-empty";
    row.textContent = "(no output \u2014 the program displayed nothing)";
    output.appendChild(row);
  }
  for (const line of response.outputs) {
    const row = document.createElement("div");
    row.className = "run-line";
    row.textContent = line;
    output.appendChild(row);
  }
  drawState(response.state);
}

function drawState(state) {
  const host = $("run-state");
  host.innerHTML = "";
  if (!state) return;

  const rows = [["Ans", state.ans], ...Object.entries(state.memories).map(
    ([name, value]) => [name, value],
  )];
  for (const [name, value] of rows) {
    if (!value) continue;
    const row = document.createElement("div");
    row.className = "state-row";
    // A memory the program never touched holds 0, which is not worth showing.
    const empty = !value.complex && value.re === 0;
    row.innerHTML =
      `<span class="state-name">${name}</span>` +
      `<span class="state-value${empty ? " zero" : ""}">${escapeHtml(value.display)}</span>`;
    host.appendChild(row);
  }
}

/** Comma- or space-separated numbers for the `?` prompts. */
function parseInputs(text) {
  return text
    .split(/[\s,]+/)
    .map((part) => part.trim())
    .filter(Boolean)
    .map(Number)
    .filter((value) => Number.isFinite(value));
}

/** Run the program's embedded `#tests` table. */
function runTests() {
  const response = fx.call({ op: "tests", entry: ENTRY, files });
  const host = $("run-output");
  host.innerHTML = "";

  if (!response.ok) {
    const row = document.createElement("div");
    row.className = "run-error";
    row.textContent = response.error.message;
    host.appendChild(row);
    return;
  }

  const header = document.createElement("div");
  header.className = response.success ? "run-line" : "run-error";
  header.textContent = `${response.passed} passed, ${response.failed} failed`;
  host.appendChild(header);

  for (const test of response.cases) {
    const row = document.createElement("div");
    row.className = test.passed ? "test pass" : "test fail";
    const detail = test.passed
      ? test.expected
      : `expected ${test.expected}, got ${test.actual}`;
    row.textContent = `${test.passed ? "\u2713" : "\u2717"} ${test.name} \u2014 ${detail}`;
    host.appendChild(row);
  }
}

// ---------------------------------------------------------------------------
// Files and examples

function drawFileList() {
  const host = $("files");
  host.innerHTML = "";
  for (const path of Object.keys(files)) {
    const button = document.createElement("button");
    button.className = `file${path === current ? " active" : ""}`;
    button.textContent = path;
    button.addEventListener("click", () => selectFile(path));
    host.appendChild(button);
  }
  $("editor-title").textContent = current;
}

function selectFile(path) {
  if (path === current) return;
  files[current] = currentText();
  current = path;
  setEditorText(files[path] ?? "");
  drawFileList();
}

function drawExampleList() {
  const select = $("examples");
  select.innerHTML = "";
  for (const example of examples) {
    const option = document.createElement("option");
    option.value = example.name;
    option.textContent = example.summary
      ? `${example.name} \u2014 ${example.summary}`
      : example.name;
    select.appendChild(option);
  }
  select.addEventListener("change", () => loadExample(select.value));
}

function loadExample(name) {
  const example = examples.find((entry) => entry.name === name);
  if (!example) return;
  // Reset to the library files plus this example as the entry document, so an
  // example's `#include`s resolve exactly as they do on disk.
  for (const key of Object.keys(files)) delete files[key];
  Object.assign(files, libraries, { [ENTRY]: example.source });
  current = ENTRY;
  setEditorText(example.source);
  drawFileList();
  $("examples").value = name;
}

function renderConstantList() {
  const response = fx.constants();
  const host = $("constants");
  for (const constant of response.constants) {
    const row = document.createElement("div");
    row.className = "constant";
    row.innerHTML =
      `<span class="constant-symbol">${escapeHtml(constant.symbol)}</span>` +
      `<span class="constant-name">phys.${escapeHtml(constant.name)}</span>` +
      `<span class="constant-value">${escapeHtml(String(constant.value))}</span>` +
      `<span class="constant-unit">${escapeHtml(constant.unit)}</span>`;
    row.title = `${constant.description} (menu ${constant.code})`;
    host.appendChild(row);
  }
}

function escapeHtml(text) {
  return String(text).replace(
    /[&<>"']/g,
    (character) =>
      ({
        "&": "&amp;",
        "<": "&lt;",
        ">": "&gt;",
        '"': "&quot;",
        "'": "&#39;",
      })[character],
  );
}

// ---------------------------------------------------------------------------
// Wiring

function wire() {
  $("run").addEventListener("click", run);
  $("run-tests").addEventListener("click", runTests);
  for (const tab of document.querySelectorAll("[data-tab]")) {
    tab.addEventListener("click", () => selectTab(tab.dataset.tab));
  }
}

function selectTab(name) {
  for (const tab of document.querySelectorAll("[data-tab]")) {
    tab.classList.toggle("active", tab.dataset.tab === name);
  }
  for (const panel of document.querySelectorAll("[data-panel]")) {
    panel.hidden = panel.dataset.panel !== name;
  }
}

// A small handle for the test suite: it drives the page the way a user does,
// then reads the DOM.
window.__fx50 = {
  setSource: (text) => {
    // Reset to a single-file project, which is what a test wants to reason
    // about: no library files, so nothing resolves unless the test says so.
    for (const key of Object.keys(files)) delete files[key];
    files[ENTRY] = text;
    current = ENTRY;
    setEditorText(text);
    drawFileList();
  },
  refresh,
  run,
  runTests,
  get files() {
    return files;
  },
  transpiled: () => $("prgm").textContent,
  sizeText: () => $("size-text").textContent,
  outputs: () => [...document.querySelectorAll(".run-line")].map((n) => n.textContent),
  diagnostics: () =>
    [...document.querySelectorAll(".diagnostic")].map((n) => n.textContent),
};

wire();
drawExampleList();
main();
