#!/usr/bin/env bun
/**
 * Build the Rust crates to WebAssembly and put the module where the site serves
 * it from.
 *
 * This is **separate from the frontend build on purpose**. The wasm is the part
 * that changes when the Rust side changes, which is rare; the page changes
 * constantly. Keeping them apart means iterating on the frontend needs no Rust
 * toolchain at all, and CI can cache the two independently:
 *
 *   bun run build:wasm   # only after touching crates/ or src/
 *   bun run build        # every time; requires the wasm, never builds it
 *
 * Prerequisites, once:
 *
 *   rustup target add wasm32-unknown-unknown
 */
import { $ } from 'bun';
import { copyFile, mkdir, stat } from 'node:fs/promises';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const web = dirname(here);
const root = dirname(web);

/** Must match `[profile.wasm]` in the workspace `Cargo.toml`. */
const PROFILE = 'wasm';
const TARGET = 'wasm32-unknown-unknown';
const CRATE = 'fx-wasm';
const ARTIFACT = 'fx_wasm.wasm';

const built = join(root, 'target', TARGET, PROFILE, ARTIFACT);
const destination = join(web, 'static', ARTIFACT);

await mkdir(dirname(destination), { recursive: true });

try {
	await $`cargo build --profile ${PROFILE} --target ${TARGET} -p ${CRATE}`.cwd(root);
} catch {
	// The usual first-run failure is the target not being installed, and
	// cargo's own message buries that among the rest.
	console.error(
		[
			'',
			'cargo could not build the WebAssembly module.',
			'',
			'If this is the first time, the target needs to be installed:',
			'',
			`    rustup target add ${TARGET}`,
			''
		].join('\n')
	);
	process.exit(1);
}

await copyFile(built, destination);

const raw = new Uint8Array(await Bun.file(destination).arrayBuffer());
const gzipped = Bun.gzipSync(raw).length;
const kb = (bytes: number) => `${(bytes / 1024).toFixed(0)} KB`;

console.log(`${relative(root, destination)}: ${kb(raw.length)} (${kb(gzipped)} gzipped)`);
