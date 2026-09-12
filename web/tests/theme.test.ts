/**
 * Tests for the theme registry.
 *
 * The palettes live in CSS and the list lives in TypeScript, so the useful thing
 * to test is that the two agree. A theme whose block is missing a variable does
 * not fail loudly — the utility reading it falls back to an unstyled default —
 * and a theme whose class is misspelled in one of the two files simply never
 * appears. Both are invisible in review and easy to introduce, so they are
 * checked here rather than trusted.
 */

import { describe, test } from 'bun:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

import {
	DEFAULT_DARK,
	DEFAULT_LIGHT,
	REQUIRED_PALETTE_VARS,
	THEMES,
	counterpartOf,
	isThemeId,
	themeById,
	type ThemeId
} from '../src/lib/theme';

const css = readFileSync(new URL('../src/routes/layout.css', import.meta.url), 'utf8');

/** Every `.theme-x { … }` block in the stylesheet, by class name. */
function cssThemeBlocks(): Map<string, string> {
	const blocks = new Map<string, string>();
	const pattern = /\.(theme-[a-z0-9-]+)\s*\{([^}]*)\}/g;
	for (const match of css.matchAll(pattern)) {
		// A theme class appears once. If it ever appears twice the later block
		// would win in the cascade, so merging is the honest reading.
		const [, name, body] = match;
		blocks.set(name, (blocks.get(name) ?? '') + body);
	}
	return blocks;
}

describe('the registry and the stylesheet agree', () => {
	test('every listed theme has a palette block', () => {
		const blocks = cssThemeBlocks();
		for (const entry of THEMES) {
			assert.ok(blocks.has(entry.id), `${entry.id} is listed but has no block in layout.css`);
		}
	});

	test('every palette block is a listed theme', () => {
		const listed = new Set<string>(THEMES.map((entry) => entry.id));
		for (const name of cssThemeBlocks().keys()) {
			assert.ok(listed.has(name), `${name} is defined in layout.css but missing from THEMES`);
		}
	});

	test('every theme defines every palette variable', () => {
		const blocks = cssThemeBlocks();
		for (const entry of THEMES) {
			const body = blocks.get(entry.id) ?? '';
			const missing = REQUIRED_PALETTE_VARS.filter((name) => !body.includes(`${name}:`));
			assert.deepEqual(missing, [], `${entry.id} is missing ${missing.join(', ')}`);
		}
	});

	test('every theme declares a matching color-scheme', () => {
		const blocks = cssThemeBlocks();
		for (const entry of THEMES) {
			const body = blocks.get(entry.id) ?? '';
			const expected = entry.dark ? 'color-scheme: dark' : 'color-scheme: light';
			assert.ok(body.includes(expected), `${entry.id} should declare \`${expected}\``);
		}
	});

	test('the mapping block turns the palette into Tailwind colours', () => {
		// The whole scheme rests on this: the utilities read `--color-*`, so if the
		// mapping stopped pointing at the palette, every theme would render as
		// Tailwind's defaults and the picker would appear to do nothing.
		for (const shade of [950, 900, 800, 700, 600, 500, 400, 300, 200, 100, 50]) {
			assert.match(
				css,
				new RegExp(`--color-neutral-${shade}:\\s*var\\(--t-`),
				`--color-neutral-${shade} should come from the palette`
			);
		}
	});
});

describe('the theme list', () => {
	test('ids are unique, prefixed and lower-case', () => {
		const seen = new Set<string>();
		for (const entry of THEMES) {
			assert.ok(entry.id.startsWith('theme-'), `${entry.id} should start with \`theme-\``);
			assert.equal(entry.id, entry.id.toLowerCase(), `${entry.id} should be lower-case`);
			assert.ok(!seen.has(entry.id), `${entry.id} is listed twice`);
			seen.add(entry.id);
		}
	});

	test('every theme has a label', () => {
		for (const entry of THEMES) {
			assert.ok(entry.label.trim().length > 0, `${entry.id} has no label`);
		}
	});

	test('both house themes exist with the right appearance', () => {
		assert.ok(isThemeId(DEFAULT_DARK));
		assert.ok(isThemeId(DEFAULT_LIGHT));
		assert.equal(themeById(DEFAULT_DARK).dark, true);
		assert.equal(themeById(DEFAULT_LIGHT).dark, false);
	});

	test('there is a Catppuccin port for each of its four flavours', () => {
		const names = THEMES.map((entry) => entry.id);
		for (const flavour of ['latte', 'frappe', 'macchiato', 'mocha']) {
			assert.ok(
				names.includes(`theme-catppuccin-${flavour}` as ThemeId),
				`Catppuccin ${flavour} is missing`
			);
		}
	});
});

describe('isThemeId', () => {
	test('accepts every listed theme', () => {
		for (const entry of THEMES) assert.ok(isThemeId(entry.id));
	});

	test('rejects anything else, including nothing', () => {
		assert.equal(isThemeId('theme-nope'), false);
		assert.equal(isThemeId('dark'), false);
		assert.equal(isThemeId(''), false);
		assert.equal(isThemeId(null), false);
		assert.equal(isThemeId(undefined), false);
	});
});

describe('counterpartOf', () => {
	test('a dark house theme flips to the light one', () => {
		assert.equal(counterpartOf('theme-fx50-dark'), 'theme-fx50-light');
	});

	test('a light theme flips back to the dark house theme', () => {
		assert.equal(counterpartOf('theme-fx50-light'), 'theme-fx50-dark');
	});

	test('Catppuccin keeps its family, in both directions', () => {
		assert.equal(counterpartOf('theme-catppuccin-mocha'), 'theme-catppuccin-latte');
		assert.equal(counterpartOf('theme-catppuccin-latte'), 'theme-catppuccin-mocha');
		// The other two flavours are dark, and share Latte as their light side.
		assert.equal(counterpartOf('theme-catppuccin-frappe'), 'theme-catppuccin-latte');
		assert.equal(counterpartOf('theme-catppuccin-macchiato'), 'theme-catppuccin-latte');
	});

	test('Gruvbox keeps its family', () => {
		assert.equal(counterpartOf('theme-gruvbox-dark'), 'theme-gruvbox-light');
		assert.equal(counterpartOf('theme-gruvbox-light'), 'theme-gruvbox-dark');
	});

	test('Solarized does not, because its two halves are both listed', () => {
		// They are independent themes rather than a family: flipping Solarized Dark
		// gives the house light theme, not Solarized Light.
		assert.equal(counterpartOf('theme-solarized-light'), 'theme-fx50-dark');
		assert.equal(counterpartOf('theme-solarized-dark'), 'theme-fx50-light');
	});

	test('a dark theme with no light twin falls back to the house light theme', () => {
		// Nord and Dracula have no light variant, and inventing one would be a lie.
		assert.equal(counterpartOf('theme-nord'), 'theme-fx50-light');
		assert.equal(counterpartOf('theme-dracula'), 'theme-fx50-light');
		assert.equal(counterpartOf('theme-tokyo-night'), 'theme-fx50-light');
		assert.equal(counterpartOf('theme-rose-pine'), 'theme-fx50-light');
	});

	test('flipping always lands on a theme, and always changes appearance', () => {
		for (const entry of THEMES) {
			const other = counterpartOf(entry.id);
			assert.ok(isThemeId(other), `${entry.id} flipped to an unknown theme`);
			assert.notEqual(
				themeById(other).dark,
				entry.dark,
				`${entry.id} flipped to another ${entry.dark ? 'dark' : 'light'} theme`
			);
		}
	});
});
