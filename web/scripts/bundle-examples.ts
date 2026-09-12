#!/usr/bin/env bun
/**
 * Generate the example programs into a module the site can import.
 *
 * The examples are read from `../examples/*.fxc` and `../examples/*.fx` — the
 * same files the Rust test suite runs — rather than copied into the page. A
 * copy would be free to drift; this cannot, and `bun run build:examples`
 * re-reads them every time.
 *
 * This needs no Rust toolchain: it only reads text files and writes one module.
 *
 * Where they go is a deliberate choice: the examples belong to **documentation**,
 * not to the editor. The editor's workspace holds the user's own program and
 * nothing else, so opening the workbench never lands you in a folder full of
 * files you did not write.
 *
 * Two shapes come out of this. `examples` and `libraries` are the `.fxc` files
 * the editor tests already consume. `programs` is every example — both
 * languages, with the embedded `#tests` parsed out — which the documentation
 * pages render. Keeping the first two exactly as they were means the docs can
 * grow without disturbing the editor's tests.
 */
import { mkdir } from 'node:fs/promises';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const web = dirname(here);
const root = dirname(web);
const destination = join(web, 'src', 'lib', 'examples.generated.ts');

/** Which of the two source languages an example is written in. */
export type Language = 'fxc' | 'prgm';

/** One case from a program's embedded `#tests` table. */
export interface TestCase {
	/** The case's description, e.g. `5! = 120`. */
	name: string;
	/** Values fed to the program's `?` prompts, if any. */
	input?: number[];
	/** The display lines the case expects. */
	output?: string[];
	/** The error the case expects instead of output. */
	error?: string;
}

/** One example file, as written in `examples/`. */
export interface Program {
	/** The repository path, e.g. `examples/arrays.fxc`. */
	path: string;
	/** The file name without its extension. */
	name: string;
	/** `.fxc` (C-like) or `.fx` (calculator PRGM). */
	language: Language;
	/** The program's first comment line, or an empty string. */
	summary: string;
	/** The whole program, untouched. */
	source: string;
	/** The parsed `#tests` table, or `null` when the program carries none. */
	tests: TestCase[] | null;
}

/** The first `//` line of a program, as a one-line description. */
function summary(source: string): string {
	for (const line of source.split('\n')) {
		const trimmed = line.trim();
		if (trimmed.startsWith('//')) {
			const body = trimmed.replace(/^\/+/, '').trim();
			// Skip banner rules and section headings — the first one of those is
			// never a useful description.
			if (body && !/^[-=]+$/.test(body) && !body.endsWith(':')) return body;
		} else if (trimmed) {
			break;
		}
	}
	return '';
}

/**
 * Parse the JSON-like `#tests` table a `.fxc` program carries.
 *
 * It is JSON with trailing commas allowed, which is exactly what a reader
 * writes, so the commas are stripped and the rest handed to `JSON.parse`. If a
 * table is present but malformed the build should stop: the transpiler would
 * reject the same file, and a docs page quietly missing its tests would be
 * worse than a failure here.
 */
function extractTests(source: string, path: string): TestCase[] | null {
	const marker = source.indexOf('#tests');
	if (marker === -1) return null;

	const start = source.indexOf('[', marker);
	if (start === -1) throw new Error(`${path}: '#tests' is not followed by an array`);

	let depth = 0;
	let inString = false;
	let escaped = false;
	let end = -1;
	for (let i = start; i < source.length; i++) {
		const character = source[i];
		if (inString) {
			if (escaped) escaped = false;
			else if (character === '\\') escaped = true;
			else if (character === '"') inString = false;
			continue;
		}
		if (character === '"') inString = true;
		else if (character === '[' || character === '{') depth++;
		else if (character === ']' || character === '}') {
			depth--;
			if (depth === 0) {
				end = i + 1;
				break;
			}
		}
	}
	if (end === -1) throw new Error(`${path}: '#tests' array is not closed`);

	const json = source.slice(start, end).replace(/,(\s*[\]}])/g, '$1');
	try {
		return JSON.parse(json) as TestCase[];
	} catch (cause) {
		throw new Error(`${path}: '#tests' is not valid JSON: ${(cause as Error).message}`);
	}
}

async function collect(pattern: string): Promise<string[]> {
	const paths: string[] = [];
	for await (const path of new Bun.Glob(pattern).scan({ cwd: root })) paths.push(path);
	return paths.sort();
}

const libraries: Record<string, string> = {};
for (const path of await collect('examples/lib/*.fxc')) {
	// Keyed by their bare name because a program's `#include "lib/…"` resolves
	// relative to the entry document, which is what these paths are written for.
	libraries[path.replace(/^examples\//, '')] = await Bun.file(join(root, path)).text();
}

const programPaths = [
	...(await collect('examples/*.fxc')),
	...(await collect('examples/*.fx'))
].sort();

const programs: Program[] = [];
for (const path of programPaths) {
	const source = await Bun.file(join(root, path)).text();
	const language: Language = path.endsWith('.fx') ? 'prgm' : 'fxc';
	programs.push({
		path,
		name: path.replace(/^examples\//, '').replace(/\.(fxc|fx)$/, ''),
		language,
		summary: summary(source),
		// The embedded `#tests` table comes along inside `source` untouched.
		source,
		tests: language === 'fxc' ? extractTests(source, path) : null
	});
}

/** The `.fxc` programs the editor tests consume, unchanged in shape. */
const fxc = programs.filter((program) => program.language === 'fxc');

const header = `// Generated by web/scripts/bundle-examples.ts from examples/*.fxc and
// examples/*.fx. Do not edit — run \`bun run build:examples\` instead.
//
// Read by the documentation pages, not by the editor. The editor's workspace is
// the user's own file and nothing else.
`;

const body = `
/** Which of the two source languages an example is written in. */
export type Language = 'fxc' | 'prgm';

/** One case from a program's embedded \`#tests\` table. */
export interface TestCase {
	/** The case's description, e.g. \`5! = 120\`. */
	name: string;
	/** Values fed to the program's \`?\` prompts, if any. */
	input?: number[];
	/** The display lines the case expects. */
	output?: string[];
	/** The error the case expects instead of output. */
	error?: string;
}

/** One example file, as written in \`examples/\`. */
export interface Program {
	/** The repository path, e.g. \`examples/arrays.fxc\`. */
	path: string;
	/** The file name without its extension. */
	name: string;
	/** \`.fxc\` (C-like) or \`.fx\` (calculator PRGM). */
	language: Language;
	/** The program's first comment line, or an empty string. */
	summary: string;
	/** The whole program, untouched. */
	source: string;
	/** The parsed \`#tests\` table, or \`null\` when the program carries none. */
	tests: TestCase[] | null;
}

/** The libraries under \`examples/lib/\`, keyed by the path an \`#include\` uses. */
export const libraries: Record<string, string> = ${JSON.stringify(libraries, null, '\t')};

/** Every top-level example, in path order. */
export const programs: Program[] = ${JSON.stringify(programs, null, '\t')};

/** The \`.fxc\` examples alone — what the editor tests iterate. */
export const examples: Program[] = programs.filter((program) => program.language === 'fxc');

/** @deprecated Use {@link Program}; kept so older imports keep their meaning. */
export type Example = Program;
`;

await mkdir(dirname(destination), { recursive: true });
await Bun.write(destination, header + body);

console.log(
	`${relative(root, destination)}: ${programs.length} example(s) ` +
		`(${fxc.length} .fxc, ${programs.length - fxc.length} .fx), ` +
		`${Object.keys(libraries).length} library file(s)`
);
