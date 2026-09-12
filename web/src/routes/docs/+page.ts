/**
 * The docs index.
 *
 * `docs` is generated at build time by `web/scripts/bundle-docs.ts`, so this
 * load runs no markdown renderer: the bodies are already HTML and the metadata
 * is already plain data. Grouping happens here rather than in the component so
 * the page only has to draw a list.
 */
import { docs } from '$lib/docs.generated';

// The whole site is prerendered; see `src/routes/+layout.ts`. Stating it here
// as well keeps the route honest if it is ever moved.
export const prerender = true;

/** One entry in the index. */
export interface DocLink {
	slug: string;
	title: string;
	description: string;
}

/** One section of the index. */
export interface DocGroup {
	name: string;
	docs: DocLink[];
}

export function load() {
	// The generated array is already in navigation order, so the first time a
	// group appears is where it belongs.
	const groups = new Map<string, DocLink[]>();
	for (const doc of docs) {
		const entries = groups.get(doc.group) ?? [];
		entries.push({ slug: doc.slug, title: doc.title, description: doc.description });
		groups.set(doc.group, entries);
	}

	return {
		groups: [...groups].map(([name, entries]) => ({ name, docs: entries })),
		total: docs.length
	};
}
