import tailwindcss from '@tailwindcss/vite';
import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

/**
 * Where the site will be served from.
 *
 * GitHub Pages serves a *project* site under `/<repo>/`, so every asset has to
 * be requested from there — `<script src="/_app/…">` would 404. The value is
 * only known when deploying, so it comes from the environment:
 *
 *   BASE_PATH=/casio-fx50fh2-emulator bun run build
 *
 * Locally it is empty and the site is served from `/`, which is what `bun run
 * dev` and `bun run preview` expect. Nothing else in the project needs to know:
 * SvelteKit rewrites its own asset URLs, and application code reads the value
 * back through `base` from `$app/paths` rather than hard-coding it.
 */
function basePath(): '' | `/${string}` {
	const value = process.env.BASE_PATH ?? '';
	if (value === '') return '';
	// A missing leading slash produces a site that builds cleanly and 404s
	// everywhere, so it is worth refusing rather than debugging later.
	if (!value.startsWith('/')) {
		throw new Error(
			`BASE_PATH must start with "/" — it is a path, not a URL. Got ${JSON.stringify(value)}.`
		);
	}
	return value as `/${string}`;
}

const base = basePath();

export default defineConfig({
	plugins: [
		tailwindcss(),
		sveltekit({
			compilerOptions: {
				// Force runes mode for the project, except for libraries. Can be removed in svelte 6.
				runes: ({ filename }) =>
					filename.split(/[/\\]/).includes('node_modules') ? undefined : true
			},
			// Emits plain files — HTML, JS, CSS — for GitHub Pages. There is no
			// server, so every route has to be prerenderable; see `+layout.ts`.
			adapter: adapter(),
			paths: { base }
		})
	]
});
