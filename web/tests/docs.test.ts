/**
 * Tests for the documentation bundle, run under `bun test`.
 *
 * Two things are checked. The pure helpers in `scripts/bundle-docs.ts` are
 * driven with strings — no DOM, no files — so link rewriting and heading ids
 * are pinned directly. Then the generated `docs.generated.ts` is checked
 * against what it claims to be: every document present, every example present,
 * slugs unique and URL-safe, and the markdown actually converted rather than
 * left as text.
 *
 * The examples are compared against the files on disk, not against a second
 * hand-written list, so the test fails if `examples/` and the bundle drift.
 *
 * Assertions use `node:assert`, matching the wasm and workspace suites.
 */

import { describe, test } from 'bun:test';
import assert from 'node:assert/strict';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import {
	DOCUMENTS,
	extractDescription,
	extractTitle,
	rewriteHref,
	renderMarkdown,
	slugFor,
	slugifyHeading
} from '../scripts/bundle-docs';
import { docs, docsBySlug, exampleAnchor } from '../src/lib/docs.generated';
import { libraries, programs } from '../src/lib/examples.generated';

const here = dirname(fileURLToPath(import.meta.url));
const repository = join(here, '..', '..');

/** Every file matching a repository-relative glob, sorted. */
async function glob(pattern: string): Promise<string[]> {
	const paths: string[] = [];
	for await (const path of new Bun.Glob(pattern).scan({ cwd: repository })) paths.push(path);
	return paths.sort();
}

/** The slugs the site is expected to publish. */
const EXPECTED_SLUGS = [
	'fxc',
	'language',
	'ai-agents',
	'architecture',
	'decisions',
	'plan',
	'readme'
];

describe('slugFor', () => {
	test('lower-cases and hyphenates a file name', () => {
		assert.equal(slugFor('docs/FXC.md'), 'fxc');
		assert.equal(slugFor('docs/AI-AGENTS.md'), 'ai-agents');
		assert.equal(slugFor('README.md'), 'readme');
		assert.equal(slugFor('docs/LANGUAGE.md'), 'language');
	});
});

describe('extractTitle / extractDescription', () => {
	test('reads the first heading as plain text', () => {
		assert.equal(extractTitle('# The `.fxc` language\n\nBody.\n'), 'The .fxc language');
		assert.equal(extractTitle('no heading here'), '');
	});

	test('takes the first paragraph after the title', () => {
		const markdown = '# Title\n\nSome **bold** and `code` text.\n\nA second paragraph.';
		assert.equal(extractDescription(markdown), 'Some bold and code text.');
	});

	test('shortens a long paragraph on a word boundary', () => {
		const description = extractDescription(`# T\n\n${'word '.repeat(80)}`);
		assert.ok(description.length <= 181, `too long: ${description.length}`);
		assert.ok(description.endsWith('…'));
	});
});

describe('slugifyHeading', () => {
	test('matches GitHub for an ADR heading with an em dash', () => {
		assert.equal(
			slugifyHeading('ADR 0032 — Dependencies are a judgement, not a rule'),
			'adr-0032--dependencies-are-a-judgement-not-a-rule'
		);
	});

	test('suffixes a repeated heading', () => {
		const seen = new Map<string, number>();
		assert.equal(slugifyHeading('Notes', seen), 'notes');
		assert.equal(slugifyHeading('Notes', seen), 'notes-1');
	});
});

describe('rewriteHref', () => {
	// Every route in this section is a directory (`docs/+layout.ts` sets
	// `trailingSlash = 'always'`), so a link ends in a slash and any anchor
	// follows that slash rather than replacing it.
	test('maps a bundled document to its route', () => {
		assert.equal(rewriteHref('FXC.md', 'docs/LANGUAGE.md'), '/docs/fxc/');
		assert.equal(rewriteHref('../README.md#build', 'docs/FXC.md'), '/docs/readme/#build');
		assert.equal(rewriteHref('docs/DECISIONS.md', 'README.md'), '/docs/decisions/');
	});

	test('bakes in a base path', () => {
		assert.equal(
			rewriteHref('LANGUAGE.md', 'docs/FXC.md', '/casio-fx50fh2-emulator'),
			'/casio-fx50fh2-emulator/docs/language/'
		);
	});

	test('points examples at the examples page', () => {
		assert.equal(
			rewriteHref('../examples/packing.fxc', 'docs/FXC.md'),
			'/docs/examples/#example-examples-packing-fxc'
		);
		assert.equal(rewriteHref('examples', 'README.md'), '/docs/examples/#example-examples');
	});

	test('sends other repository files to GitHub and leaves absolute URLs alone', () => {
		const readme = rewriteHref('crates/fx-transpiler/README.md', 'README.md');
		assert.match(
			readme,
			/^https:\/\/github\.com\/.+\/blob\/master\/crates\/fx-transpiler\/README\.md$/
		);
		assert.equal(rewriteHref('https://example.com/x', 'README.md'), 'https://example.com/x');
		assert.equal(rewriteHref('#anchors', 'docs/FXC.md'), '#anchors');
	});
});

describe('renderMarkdown', () => {
	test('converts headings and gives them ids', () => {
		const html = renderMarkdown('# Hi there\n\nA paragraph.\n', 'docs/x.md');
		assert.ok(html.includes('<h1 id="hi-there">Hi there</h1>'), html);
		assert.ok(html.includes('<p>A paragraph.</p>'), html);
	});

	test('does not leave a heading marker behind', () => {
		assert.ok(!/^# /m.test(renderMarkdown('# Hi\n\nBody\n', 'docs/x.md')));
	});
});

describe('the generated documents', () => {
	test('every expected document is present, with a title and a body', () => {
		assert.deepEqual(docs.map((doc) => doc.slug).sort(), [...EXPECTED_SLUGS].sort());
		for (const doc of docs) {
			assert.ok(doc.title.length > 0, `${doc.slug} has no title`);
			assert.ok(doc.html.length > 0, `${doc.slug} has no body`);
			assert.ok(doc.description.length > 0, `${doc.slug} has no description`);
			assert.ok(htmlText(doc.html).length > 0, `${doc.slug} has no visible text`);
		}
	});

	test('slugs are unique and URL-safe', () => {
		const slugs = docs.map((doc) => doc.slug);
		assert.equal(new Set(slugs).size, slugs.length, 'duplicate slug');
		for (const slug of slugs) {
			assert.match(slug, /^[a-z0-9]+(?:-[a-z0-9]+)*$/, `unsafe slug: ${slug}`);
		}
	});

	test('docsBySlug agrees with docs and the source registry', () => {
		assert.deepEqual(Object.keys(docsBySlug).sort(), docs.map((doc) => doc.slug).sort());
		assert.deepEqual(
			docs.map((doc) => doc.sourcePath).sort(),
			DOCUMENTS.map((spec) => spec.file).sort()
		);
		for (const doc of docs) assert.equal(docsBySlug[doc.slug], doc);
	});

	test('the markdown was rendered, not left as text', () => {
		for (const doc of docs) {
			assert.ok(/<h1[ >]/.test(doc.html), `${doc.slug} has no rendered h1`);
			// Code fences may legitimately contain a `# ` line (a shell comment,
			// a preprocessor directive), so they are removed before looking for a
			// heading marker that was never converted.
			const outsideCode = doc.html
				.replace(/<pre[\s\S]*?<\/pre>/g, '')
				.replace(/<code[\s\S]*?<\/code>/g, '');
			assert.ok(!/^# /m.test(outsideCode), `${doc.slug} contains a raw '# ' heading`);
		}
	});
});

describe('the generated examples', () => {
	test('every top-level example in examples/ is bundled, source and all', async () => {
		const paths = [...(await glob('examples/*.fxc')), ...(await glob('examples/*.fx'))].sort();
		assert.ok(paths.length > 0, 'no examples found on disk');

		const bundled = new Map(programs.map((program) => [program.path, program]));
		assert.equal(bundled.size, paths.length, 'bundle has a different number of examples');

		for (const path of paths) {
			const program = bundled.get(path);
			assert.ok(program, `${path} is missing from programs`);
			assert.equal(
				program.source,
				await Bun.file(join(repository, path)).text(),
				`${path} source drifted`
			);
			assert.equal(program.language, path.endsWith('.fx') ? 'prgm' : 'fxc');
			if (program.language === 'fxc') {
				assert.ok(program.tests, `${path} should carry parsed tests`);
			}
		}
	});

	test('both languages are represented', () => {
		assert.ok(
			programs.some((program) => program.language === 'fxc'),
			'no .fxc examples'
		);
		assert.ok(
			programs.some((program) => program.language === 'prgm'),
			'no .fx examples'
		);
	});

	test('every library under examples/lib/ is bundled', async () => {
		const paths = await glob('examples/lib/*.fxc');
		assert.ok(paths.length > 0, 'no libraries found on disk');
		for (const path of paths) {
			const key = path.replace(/^examples\//, '');
			assert.ok(key in libraries, `${path} is missing from libraries`);
			assert.equal(
				libraries[key],
				await Bun.file(join(repository, path)).text(),
				`${path} source drifted`
			);
		}
		assert.equal(Object.keys(libraries).length, paths.length);
	});

	test('example anchors are unique and agree with the helper', () => {
		const anchors = programs.map((program) => exampleAnchor(program.path));
		assert.equal(new Set(anchors).size, anchors.length, 'duplicate example anchor');
		assert.equal(exampleAnchor('examples/factorial.fxc'), 'example-examples-factorial-fxc');
	});
});

/** The visible text of an HTML fragment, with tags and entities removed. */
function htmlText(html: string): string {
	return html
		.replace(/<[^>]*>/g, ' ')
		.replace(/&[a-z]+;|&#\d+;/gi, ' ')
		.replace(/\s+/g, ' ')
		.trim();
}
