#!/usr/bin/env bun
/**
 * Render the repository's markdown into a module the site can import.
 *
 * The documentation lives in `docs/*.md` and the root `README.md`, and it is
 * read here — once, at build time — rather than fetched in the browser. That is
 * the whole point: the pages are prerendered to static HTML, so a reader with
 * JavaScript off, or a search engine, sees the same words. It also means the
 * markdown is rendered by the `marked` package in this script and never by a
 * runtime markdown library shipped to the client.
 *
 * `bun run build:docs` writes `web/src/lib/docs.generated.ts`. The pages import
 * that; nothing imports `marked` at runtime.
 *
 * Links are rewritten while the tokens are still tokens, not afterwards on the
 * HTML: a link between two bundled documents points at `/docs/<slug>`, a link
 * into `examples/` at the examples page, and a link to anything else in the
 * repository at its GitHub source. That last case exists because the markdown
 * was written to be read on GitHub, where `../crates/fx-transpiler/README.md`
 * resolves; on a static site it would 404.
 */
import { existsSync, statSync } from 'node:fs';
import { mkdir } from 'node:fs/promises';
import { dirname, join, posix } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Marked, type Token } from 'marked';
import { format, resolveConfig } from 'prettier';

const here = dirname(fileURLToPath(import.meta.url));
const web = dirname(here);
const root = dirname(web);
const destination = join(web, 'src', 'lib', 'docs.generated.ts');

/** Where a link to something outside the bundled docs is pointed. */
const repository = 'https://github.com/wqLouis/casio-fx50fh2-emulator';
const branch = 'master';

/** One document, and where its rendered form goes in the navigation. */
export interface DocSpec {
	/** Repository path, e.g. `docs/FXC.md`. */
	file: string;
	/** The sidebar section it belongs to. */
	group: string;
	/** Position within its group; lower comes first. */
	order: number;
}

/** Which documents the site publishes, and in what order. */
export const DOCUMENTS: DocSpec[] = [
	{ file: 'docs/FXC.md', group: 'Languages', order: 10 },
	{ file: 'docs/LANGUAGE.md', group: 'Languages', order: 20 },
	{ file: 'docs/AI-AGENTS.md', group: 'Guides', order: 10 },
	{ file: 'docs/ARCHITECTURE.md', group: 'Internals', order: 10 },
	{ file: 'docs/DECISIONS.md', group: 'Internals', order: 20 },
	{ file: 'docs/PLAN.md', group: 'Internals', order: 30 },
	{ file: 'README.md', group: 'Project', order: 10 }
];

/** The order the groups appear in. */
export const GROUPS = ['Languages', 'Guides', 'Internals', 'Project'];

/** A rendered document. */
export interface Doc {
	/** URL-safe identifier, e.g. `ai-agents`. */
	slug: string;
	/** The document's first `#` heading, as plain text. */
	title: string;
	/** The first paragraph, shortened for the index. */
	description: string;
	/** The navigation section. */
	group: string;
	/** Position within the group. */
	order: number;
	/** Repository path, e.g. `docs/FXC.md`. */
	sourcePath: string;
	/** The rendered body. */
	html: string;
}

/** The fragment id for one example, from its repository path. */
export function exampleAnchor(path: string): string {
	return `example-${path.replace(/[^a-zA-Z0-9]+/g, '-').toLowerCase()}`;
}

/** A slug from a markdown file name: `docs/AI-AGENTS.md` becomes `ai-agents`. */
export function slugFor(file: string): string {
	return (file.split('/').pop() ?? file)
		.replace(/\.md$/i, '')
		.toLowerCase()
		.replace(/[^a-z0-9]+/g, '-')
		.replace(/^-+|-+$/g, '');
}

/** Markdown inline syntax reduced to the words a reader sees. */
function plainText(text: string): string {
	return text
		.replace(/!?\[([^\]]*)\]\([^)]*\)/g, '$1')
		.replace(/`([^`]*)`/g, '$1')
		.replace(/\*\*([^*]*)\*\*/g, '$1')
		.replace(/\*([^*]*)\*/g, '$1')
		.replace(/<[^>]*>/g, '')
		.replace(/\s+/g, ' ')
		.trim();
}

/** The text of the first `#` heading, or an empty string. */
export function extractTitle(markdown: string): string {
	const match = markdown.match(/^#\s+(.+)$/m);
	return match ? plainText(match[1]) : '';
}

/** The first paragraph after the title, shortened for an index card. */
export function extractDescription(markdown: string, limit = 180): string {
	const lines = markdown.split('\n');
	let index = lines.findIndex((line) => /^#\s+/.test(line)) + 1;
	const paragraph: string[] = [];
	for (; index < lines.length; index++) {
		const line = lines[index];
		if (line.startsWith('```')) break;
		const blank = line.trim() === '';
		if (blank) {
			if (paragraph.length > 0) break;
			continue;
		}
		const isBlock =
			/^#{1,6}\s/.test(line) ||
			line.startsWith('>') ||
			line.startsWith('|') ||
			/^[-*+]\s/.test(line) ||
			/^\d+\.\s/.test(line);
		if (isBlock) {
			if (paragraph.length > 0) break;
			continue;
		}
		paragraph.push(line);
	}
	const text = plainText(paragraph.join(' '));
	if (text.length <= limit) return text;
	const cut = text.slice(0, limit);
	const lastSpace = cut.lastIndexOf(' ');
	return `${cut.slice(0, lastSpace > limit / 2 ? lastSpace : limit).trimEnd()}…`;
}

/** Common HTML entities decoded so a slug made from text keeps its letters. */
function decodeEntities(text: string): string {
	return text
		.replace(/&amp;/g, '&')
		.replace(/&lt;/g, '<')
		.replace(/&gt;/g, '>')
		.replace(/&quot;/g, '"')
		.replace(/&#39;/g, "'")
		.replace(/&nbsp;/g, ' ');
}

/**
 * A GitHub-style heading id, so `#adr-0032--…` links keep working.
 *
 * The algorithm is GitHub's: lower-case the text, drop everything that is not a
 * letter, number, space, underscore or hyphen, and turn each space into a
 * hyphen — without collapsing runs, which is why an em dash leaves two.
 * Duplicates get a `-1`, `-2`, … suffix.
 */
export function slugifyHeading(text: string, seen?: Map<string, number>): string {
	const base = decodeEntities(text)
		.toLowerCase()
		.replace(/[^\p{L}\p{N}\p{M}\s_-]/gu, '')
		.replace(/ /g, '-');
	if (!seen) return base;
	const count = seen.get(base) ?? 0;
	seen.set(base, count + 1);
	return count === 0 ? base : `${base}-${count}`;
}

/**
 * Point a markdown link at the right place on the static site.
 *
 * Anchors and absolute URLs are left alone. A link to a bundled document
 * becomes `/docs/<slug>/`, a link into `examples/` becomes the examples page,
 * and anything else that exists in the repository becomes a GitHub link. A link
 * that resolves to nothing is returned unchanged rather than guessed at.
 *
 * The trailing slash is not decoration: this section is built with
 * `trailingSlash = 'always'` (see `docs/+layout.ts`), so an anchor follows the
 * slash — `/docs/fxc/#the-grammar`, never `/docs/fxc#the-grammar`.
 */
export function rewriteHref(href: string, fromFile: string, base = ''): string {
	const hashAt = href.indexOf('#');
	const target = hashAt === -1 ? href : href.slice(0, hashAt);
	const hash = hashAt === -1 ? '' : href.slice(hashAt);
	if (!target) return href;
	if (/^[a-z][a-z0-9+.-]*:/i.test(target) || target.startsWith('//')) return href;

	const resolved = posix.normalize(posix.join(posix.dirname(fromFile), target));
	const repoPath = resolved.startsWith('./') ? resolved.slice(2) : resolved;

	const doc = DOCUMENTS.find((entry) => entry.file === repoPath);
	if (doc) return `${base}/docs/${slugFor(doc.file)}/${hash}`;

	if (repoPath === 'examples' || repoPath.startsWith('examples/')) {
		return `${base}/docs/examples/#${exampleAnchor(repoPath)}`;
	}

	const absolute = join(root, repoPath);
	if (existsSync(absolute)) {
		const kind = statSync(absolute).isDirectory() ? 'tree' : 'blob';
		return `${repository}/${kind}/${branch}/${repoPath}${hash}`;
	}
	return href;
}

/** Render one markdown file, rewriting links and giving headings ids. */
export function renderMarkdown(markdown: string, fromFile: string, base = ''): string {
	const parser = new Marked({
		gfm: true,
		walkTokens(token: Token) {
			if (token.type === 'link' || token.type === 'image') {
				token.href = rewriteHref(token.href, fromFile, base);
			}
		}
	});

	const seen = new Map<string, number>();
	return parser
		.parse(markdown, { async: false })
		.replace(/<h([1-6])>([\s\S]*?)<\/h\1>/g, (_match, level: string, inner: string) => {
			const text = inner.replace(/<[^>]*>/g, '');
			return `<h${level} id="${slugifyHeading(text, seen)}">${inner}</h${level}>`;
		});
}

/** Read and render every published document, in navigation order. */
export async function buildDocs(base = ''): Promise<Doc[]> {
	const docs: Doc[] = [];
	for (const entry of DOCUMENTS) {
		const markdown = await Bun.file(join(root, entry.file)).text();
		docs.push({
			slug: slugFor(entry.file),
			title: extractTitle(markdown),
			description: extractDescription(markdown),
			group: entry.group,
			order: entry.order,
			sourcePath: entry.file,
			html: renderMarkdown(markdown, entry.file, base)
		});
	}
	return docs.sort(
		(a, b) =>
			GROUPS.indexOf(a.group) - GROUPS.indexOf(b.group) ||
			a.order - b.order ||
			a.slug.localeCompare(b.slug)
	);
}

if (import.meta.main) {
	const base = process.env.BASE_PATH ?? '';
	const docs = await buildDocs(base);

	const header = `// Generated by web/scripts/bundle-docs.ts from docs/*.md and README.md.
// Do not edit — run \`bun run build:docs\` instead.
//
// Markdown is rendered here, at build time, so the pages are static HTML and
// the site ships no markdown renderer to the browser.
`;

	const body = `
/** A rendered document. */
export interface Doc {
	/** URL-safe identifier, e.g. \`ai-agents\`. */
	slug: string;
	/** The document's first \`#\` heading, as plain text. */
	title: string;
	/** The first paragraph, shortened for the index. */
	description: string;
	/** The navigation section. */
	group: string;
	/** Position within the group. */
	order: number;
	/** Repository path, e.g. \`docs/FXC.md\`. */
	sourcePath: string;
	/** The rendered body. */
	html: string;
}

/** The fragment id for one example, from its repository path. */
export function exampleAnchor(path: string): string {
	return \`example-\${path.replace(/[^a-zA-Z0-9]+/g, '-').toLowerCase()}\`;
}

/** Every published document, in navigation order. */
export const docs: Doc[] = ${JSON.stringify(docs, null, '\t')};

/** The documents keyed by slug, for the \`[slug]\` route. */
export const docsBySlug: Record<string, Doc> = Object.fromEntries(
	docs.map((doc) => [doc.slug, doc])
);
`;

	await mkdir(dirname(destination), { recursive: true });
	// `docs.generated.ts` is not in `.prettierignore`, so it is formatted as it
	// is written; that keeps `format:check` clean right after a build.
	const config = await resolveConfig(destination);
	const content = await format(header + body, { ...config, parser: 'typescript' });
	await Bun.write(destination, content);

	console.log(
		`${destination.slice(root.length + 1)}: ${docs.length} document(s), ` +
			`${docs.reduce((total, doc) => total + doc.html.length, 0)} bytes of HTML`
	);
}
