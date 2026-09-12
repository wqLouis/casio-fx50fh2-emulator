/**
 * The docs section is addressed as directories, not as files.
 *
 * SvelteKit's default (`never`) writes the index as `docs.html` while the
 * children live in a `docs/` directory, so the build contains *both*. A host
 * that resolves `/docs` onto the directory then finds no `docs/index.html` and
 * serves a listing — Python's `http.server` does exactly that, and so do many
 * static hosts. Which one wins is luck.
 *
 * `always` removes the ambiguity: the index becomes `docs/index.html` and every
 * page becomes `docs/<slug>/index.html`, which any static host resolves without
 * guessing. It is set here rather than globally because the workbench is a
 * single page with nothing to disambiguate, and `/` is already a directory.
 *
 * Every link into this section must therefore end in a slash — including the
 * ones the markdown generator bakes into the rendered documents, and the `#`
 * anchors, which follow the slash rather than replacing it.
 */
export const trailingSlash = 'always';
