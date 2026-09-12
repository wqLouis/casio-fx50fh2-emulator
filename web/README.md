# web — the browser workbench

A SvelteKit app that lets you write `.fxc`, watch it become PRGM, and run it on
the emulator. The transpiler, interpreter and editor logic are the Rust crates
from this repository, compiled to WebAssembly — **none of it is reimplemented in
JavaScript.** This side only moves text between an editor and the module, and
draws the answers.

## Getting started

```bash
rustup target add wasm32-unknown-unknown   # once, for the wasm build
cd web
bun install
bun run build:wasm                         # Rust -> web/static/fx_wasm.wasm
bun run dev
```

## The two builds are independent

```
crates/ + src/   ──bun run build:wasm──▶  web/static/fx_wasm.wasm
                                                    │
web/src/         ────bun run build────▶  web/build/ (static site)
```

The wasm only changes when the Rust side does, which is rare; the page changes
constantly. So `bun run build` **never** builds Rust — it checks that the module
is there and fails with instructions if it is not. Iterating on the frontend
needs no Rust toolchain at all, and CI can cache the two halves separately.

`bun run build:examples` needs neither toolchain: it reads `../examples/*.fxc`
(the same files the Rust test suite runs) and writes a TypeScript module.

## Commands

|                                   |                                                  |
| --------------------------------- | ------------------------------------------------ |
| `bun run dev`                     | dev server, with hot reload                      |
| `bun run build`                   | the static site, into `build/`                   |
| `bun run preview`                 | serve the built site                             |
| `bun run build:wasm`              | compile the Rust crates to `static/fx_wasm.wasm` |
| `bun run build:examples`          | regenerate `src/lib/examples.generated.ts`       |
| `bun test`                        | the end-to-end wasm tests                        |
| `bun run check`                   | `svelte-check` — types and Svelte diagnostics    |
| `bun run format` / `format:check` | Prettier, including Tailwind class order         |

## Deployment

The site is fully static: `@sveltejs/adapter-static` writes HTML, JS and CSS, and
`src/routes/+layout.ts` marks every route as prerenderable. There is no server,
which is the only shape GitHub Pages can serve.

For a **project** page (`https://<user>.github.io/<repo>/`) the site lives under
a subpath, so the build needs to be told:

```bash
BASE_PATH=/casio-fx50fh2-emulator bun run build
```

`vite.config.ts` reads that, and refuses a value that is not a path starting with
`/` — a missing slash builds cleanly and then 404s everywhere. Application code
never hard-codes a URL: it uses `base` from `$app/paths`, which SvelteKit
populates from the same setting.

Worth knowing: because SvelteKit's `paths.relative` is on by default, asset URLs
in the built HTML are relative and the base is also derived from `location` at
runtime — so the site happens to work from a subpath even without `BASE_PATH`.
Set it anyway; relying on that is a coincidence, not a contract.

`static/.nojekyll` is included so that GitHub Pages does not run Jekyll over the
output (which would ignore `_app/`).

## Layout

| Path                            |                                                           |
| ------------------------------- | --------------------------------------------------------- |
| `src/routes/`                   | the pages. `+layout.ts` sets `prerender`                  |
| `src/lib/fx50.ts`               | the wasm client: the ABI, and the types of every response |
| `src/lib/examples.generated.ts` | **generated** from `../examples/*.fxc`                    |
| `scripts/`                      | the build steps, run by bun                               |
| `tests/`                        | the end-to-end wasm tests                                 |
| `static/`                       | `fx_wasm.wasm` (**generated**), `.nojekyll`, `robots.txt` |

## Testing

```bash
bun test
```

`test` rebuilds the module first, on purpose. These tests load
`static/fx_wasm.wasm` — the artifact a browser actually downloads — and a plain
`cargo build` writes to `target/`, so without the rebuild it is easy to sit
looking at a **stale module** and draw conclusions from the wrong code. That
happened once while this was being written, which is why `test` depends on
`build:wasm` rather than trusting the file to be current. In exchange, `test` is
the one command here that needs a Rust toolchain; `dev`, `build` and
`build:examples` still do not.

This is the only place the real artifact is exercised. `cargo test` cannot catch a broken export, a wrong length prefix, or
memory freed twice; instantiating the module and calling it can. Bun has a full
`WebAssembly` implementation, so no browser is needed, and the suite includes
running the embedded `#tests` of **every** bundled example.

It has already earned its keep: it caught the test runner still reading
`#include`d libraries from the filesystem, which worked locally and failed in
wasm.

## Notes

**No `wasm-bindgen`.** The interface is a JSON string in and a JSON string out
over three `extern "C"` functions, so the only prerequisite is a rustup target —
no `wasm-bindgen` CLI at a version that must match the crate. The module is a
plain `wasm32-unknown-unknown` binary with **no imports at all** (a test asserts
it), which any host can instantiate. See
[ADR 0031](../docs/DECISIONS.md).

**CodeMirror 6**, not Monaco. It is roughly 150 KB gzipped against Monaco's
several MB, it is ESM-native so it bundles with Vite with no worker plumbing, and
its autocomplete/lint/hover APIs map one-to-one onto the module's `completions`,
`diagnostics` and `hover`. Icons are `@lucide/svelte`, imported per icon rather
than from the package root — the barrel re-exports about 1700 of them and there
is no reason for the dev server to walk all of them.

**Examples belong to the documentation, not the editor.** `bundle-examples.ts`
generates them for the docs pages; the editor's workspace holds the user's own
program and nothing else, so opening the workbench never lands you in a folder
full of files you did not write.

**Everything is a `devDependency`.** The output is fully static, so nothing is
needed at runtime; Vite bundles it all, which is why SvelteKit itself is a
`devDependency` too.

### Still to do

- The editor, and the documentation pages that consume the examples.
- A favicon — the scaffold's Svelte logo is still in place.
