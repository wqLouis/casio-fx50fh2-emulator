/**
 * Every route in this site is rendered to HTML at build time.
 *
 * This is required, not a preference: `@sveltejs/adapter-static` refuses to
 * build a route it cannot prerender, and the whole point of the adapter is to
 * emit files that a static host such as GitHub Pages can serve with no Node
 * process behind them. Without this setting the scaffold's own `bun run build`
 * fails with "Encountered dynamic routes".
 *
 * It does not mean the pages are not interactive. Components still hydrate and
 * run in the browser; there is simply no server-side request handling.
 */
export const prerender = true;
