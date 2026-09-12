/**
 * One documentation page.
 *
 * The body is HTML generated at build time, so this only has to look up the
 * document the URL names. `entries()` is what lets `adapter-static` enumerate
 * the dynamic route and write one HTML file per document.
 */
import { error } from '@sveltejs/kit';

import { docs, docsBySlug } from '$lib/docs.generated';

export const prerender = true;

/** Every slug, so the adapter can write `/docs/<slug>.html` for each. */
export function entries() {
	return docs.map((doc) => ({ slug: doc.slug }));
}

export function load({ params }) {
	const doc = docsBySlug[params.slug];
	if (!doc) error(404, `No document named “${params.slug}”.`);
	return { doc };
}
