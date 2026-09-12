#!/usr/bin/env bun
/**
 * Check that the WebAssembly module is present, and explain what to do if it is
 * not.
 *
 * Run before `dev` and `build`. It deliberately does **not** build the module:
 * the two are separate so that working on the page never needs a Rust
 * toolchain. What it does is turn a missing file — which would otherwise appear
 * as a 404 in the browser or a confusing adapter error — into a sentence that
 * says what to run.
 */
import { stat } from 'node:fs/promises';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const web = dirname(here);
const root = dirname(web);
const artifact = join(web, 'static', 'fx_wasm.wasm');

try {
	const { size } = await stat(artifact);
	if (size === 0) throw new Error('empty');
} catch {
	console.error(
		[
			'',
			'The WebAssembly module is missing, so there is nothing for the page to load.',
			'',
			'Build it once (and again whenever the Rust side changes):',
			'',
			'    bun run build:wasm',
			'',
			'That needs the Rust target, once:',
			'',
			'    rustup target add wasm32-unknown-unknown',
			''
		].join('\n')
	);
	process.exit(1);
}

console.log(`${relative(root, artifact)} is present.`);
