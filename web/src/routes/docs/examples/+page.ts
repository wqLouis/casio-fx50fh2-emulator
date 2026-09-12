/**
 * Every example, rendered from the generated module.
 *
 * `examples.generated.ts` is produced by `web/scripts/bundle-examples.ts` from
 * the same files the Rust and wasm tests run, so what the page shows cannot
 * drift from what the project actually ships. Both languages are included, and
 * each `.fxc` program's embedded `#tests` are parsed out so the page can say
 * what a program is asserted to do, not just what it looks like.
 */
import { libraries, programs } from '$lib/examples.generated';

export const prerender = true;

export function load() {
	return { programs, libraries };
}
