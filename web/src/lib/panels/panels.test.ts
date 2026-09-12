/**
 * Tests for the panel primitives, run under `bun test`.
 *
 * Only the pure modules are covered here: the clamping arithmetic, the layout
 * validation and the REPL logic. The components themselves need a browser —
 * pointer capture, `ResizeObserver`, `tick` and runes have no meaning under Bun
 * — and are exercised by `bun run check` and the built site instead.
 *
 * Assertions use `node:assert`, matching the rest of the suite.
 */

import { describe, test } from 'bun:test';
import assert from 'node:assert/strict';

import { clampPaneSize, clampRatio, defaultPaneSize, splitBounds } from './split';
import {
	emptyLayout,
	normalizeLayout,
	parseLayout,
	serializeLayout,
	withPanel,
	withSize,
	type LayoutData
} from './layout';
import {
	describeThrown,
	formatReplError,
	historyBack,
	historyForward,
	parseInputs,
	pushHistory
} from './repl';

// ---------------------------------------------------------------------------
// split.ts — the clamping arithmetic
// ---------------------------------------------------------------------------

describe('splitBounds', () => {
	test('gives the first pane the space the second minimum leaves', () => {
		assert.deepEqual(splitBounds(120, 80, 1000), { min: 120, max: 920 });
	});

	test('a zero minimum lets the first pane take the whole axis', () => {
		assert.deepEqual(splitBounds(0, 0, 100), { min: 0, max: 100 });
	});

	test('a negative minimum reads as zero rather than inverting the range', () => {
		assert.deepEqual(splitBounds(-50, -20, 100), { min: 0, max: 100 });
	});

	test('a zero container has a zero range', () => {
		assert.deepEqual(splitBounds(100, 100, 0), { min: 0, max: 0 });
	});

	test('a non-finite container is treated as zero', () => {
		assert.deepEqual(splitBounds(100, 100, Number.NaN), { min: 0, max: 0 });
		assert.deepEqual(splitBounds(100, 100, Number.POSITIVE_INFINITY), { min: 0, max: 0 });
	});

	test('too small for both minimums collapses to their ratio, inside the container', () => {
		const bounds = splitBounds(80, 80, 100);
		assert.equal(bounds.min, bounds.max);
		assert.equal(bounds.min, 50);
		assert.ok(bounds.min >= 0 && bounds.min <= 100);
	});

	test('a minimum larger than the container still lands inside it', () => {
		const bounds = splitBounds(150, 10, 100);
		assert.equal(bounds.min, bounds.max);
		assert.equal(bounds.min, 93.75);
		assert.ok(bounds.min >= 0 && bounds.min <= 100);
	});
});

describe('clampPaneSize', () => {
	test('leaves a size already inside the range alone', () => {
		assert.equal(clampPaneSize({ size: 400, min: 100, minOther: 100, total: 1000 }), 400);
	});

	test('clamps up to the first pane minimum', () => {
		assert.equal(clampPaneSize({ size: 20, min: 150, minOther: 100, total: 1000 }), 150);
	});

	test('clamps down so the second pane keeps its minimum', () => {
		assert.equal(clampPaneSize({ size: 980, min: 100, minOther: 150, total: 1000 }), 850);
	});

	test('never returns a negative size', () => {
		assert.equal(clampPaneSize({ size: -1000, min: 100, minOther: 100, total: 1000 }), 100);
	});

	test('never returns a size larger than the container', () => {
		const size = clampPaneSize({ size: 1e9, min: 100, minOther: 100, total: 1000 });
		assert.equal(size, 900);
		assert.ok(size <= 1000);
	});

	test('a non-finite proposal is clamped into range rather than propagated', () => {
		// A non-finite proposal is treated as zero and then clamped up to the
		// first pane's minimum: safe, and never an overflowing size.
		for (const size of [Number.NaN, Number.POSITIVE_INFINITY, Number.NEGATIVE_INFINITY]) {
			const clamped = clampPaneSize({ size, min: 100, minOther: 100, total: 1000 });
			assert.equal(clamped, 100);
		}
	});

	test('a container too small for both minimums still returns a finite, contained size', () => {
		for (const total of [1, 10, 50, 99]) {
			const size = clampPaneSize({ size: 500, min: 80, minOther: 80, total });
			assert.ok(Number.isFinite(size), `total ${total} gave ${size}`);
			assert.ok(size >= 0 && size <= total, `total ${total} gave ${size}`);
		}
	});

	test('a zero container returns zero', () => {
		assert.equal(clampPaneSize({ size: 500, min: 80, minOther: 80, total: 0 }), 0);
	});
});

describe('defaultPaneSize', () => {
	test('splits in half by default', () => {
		assert.equal(defaultPaneSize(1000, 100, 100), 500);
	});

	test('honours a custom ratio', () => {
		assert.equal(defaultPaneSize(1000, 100, 100, 0.25), 250);
	});

	test('clamps a ratio above one and below zero', () => {
		assert.equal(defaultPaneSize(1000, 100, 100, 5), 900);
		assert.equal(defaultPaneSize(1000, 100, 100, -5), 100);
	});

	test('a bad ratio falls back to half', () => {
		assert.equal(defaultPaneSize(1000, 100, 100, Number.NaN), 500);
	});
});

describe('clampRatio', () => {
	test('reduces to the unit interval', () => {
		assert.equal(clampRatio(-1), 0);
		assert.equal(clampRatio(0.5), 0.5);
		assert.equal(clampRatio(2), 1);
		assert.equal(clampRatio(Number.NaN), 0.5);
	});
});

// ---------------------------------------------------------------------------
// layout.ts — persistence validation
// ---------------------------------------------------------------------------

describe('layout validation', () => {
	test('a fresh layout is this build’s version and empty', () => {
		assert.deepEqual(emptyLayout(), { version: 1, panels: {}, sizes: {} });
	});

	test('a valid blob round-trips', () => {
		const value = {
			version: 1,
			panels: { run: true, problems: false },
			sizes: { main: 420, bottom: 180 }
		};
		const parsed = parseLayout(JSON.stringify(value));
		assert.deepEqual(parsed, value);
		assert.deepEqual(parseLayout(serializeLayout(parsed)), value);
	});

	test('absent, empty and whitespace-only storage reads as empty', () => {
		for (const raw of [null, undefined, '', '   ']) {
			assert.deepEqual(parseLayout(raw), emptyLayout());
		}
	});

	test('corrupt JSON reads as empty rather than throwing', () => {
		assert.deepEqual(parseLayout('{ not json'), emptyLayout());
		assert.deepEqual(parseLayout('[1, 2, 3'), emptyLayout());
		assert.deepEqual(parseLayout('null'), emptyLayout());
	});

	test('a shape from another version is discarded', () => {
		const old = JSON.stringify({ version: 0, panels: { run: true }, sizes: { main: 1 } });
		assert.deepEqual(parseLayout(old), emptyLayout());
		const future = JSON.stringify({ version: 2, panels: { run: true }, sizes: { main: 1 } });
		assert.deepEqual(parseLayout(future), emptyLayout());
	});

	test('a value that is not an object is discarded', () => {
		for (const raw of ['42', '"text"', '[1,2]', 'true']) {
			assert.deepEqual(parseLayout(raw), emptyLayout());
		}
	});

	test('bad panel and size entries are dropped, the good ones kept', () => {
		const messy = {
			version: 1,
			panels: { run: true, bad: 'yes', alsoBad: 1, '': true, problems: false },
			sizes: { main: 420, negative: -5, nan: Number.NaN, inf: Number.POSITIVE_INFINITY, text: '10' }
		};
		assert.deepEqual(normalizeLayout(messy), {
			version: 1,
			panels: { run: true, problems: false },
			sizes: { main: 420 }
		});
	});

	test('serialising a foreign version writes an empty layout, not the foreign one', () => {
		const foreign: LayoutData = { version: 9, panels: { run: true }, sizes: { main: 5 } };
		const text = serializeLayout(foreign);
		assert.deepEqual(JSON.parse(text), emptyLayout());
	});

	test('withPanel copies rather than mutating', () => {
		const before = emptyLayout();
		const after = withPanel(before, 'run', true);
		assert.deepEqual(before.panels, {});
		assert.equal(after.panels.run, true);
		assert.notEqual(before, after);
	});

	test('withPanel ignores an empty id', () => {
		const before = emptyLayout();
		assert.equal(withPanel(before, '', true), before);
	});

	test('withSize copies and ignores unusable sizes', () => {
		const before = emptyLayout();
		const after = withSize(before, 'main', 420);
		assert.deepEqual(before.sizes, {});
		assert.equal(after.sizes.main, 420);
		assert.equal(withSize(before, 'main', -1), before);
		assert.equal(withSize(before, 'main', Number.NaN), before);
		assert.equal(withSize(before, '', 10), before);
	});
});

// ---------------------------------------------------------------------------
// repl.ts — history, inputs and errors
// ---------------------------------------------------------------------------

describe('parseInputs', () => {
	test('an empty field is no inputs, not an error', () => {
		assert.deepEqual(parseInputs(''), []);
		assert.deepEqual(parseInputs('   '), []);
	});

	test('accepts spaces, commas and both together', () => {
		assert.deepEqual(parseInputs('1 2 3'), [1, 2, 3]);
		assert.deepEqual(parseInputs('1,2,3'), [1, 2, 3]);
		assert.deepEqual(parseInputs('1, 2   3'), [1, 2, 3]);
	});

	test('accepts negatives, decimals and exponents', () => {
		assert.deepEqual(parseInputs('-1.5 2e3'), [-1.5, 2000]);
	});

	test('anything that is not a number is reported rather than dropped', () => {
		assert.equal(parseInputs('abc'), null);
		assert.equal(parseInputs('1, nope'), null);
		assert.equal(parseInputs('1,,x'), null);
	});
});

describe('history', () => {
	test('pushHistory appends and moves a repeat to the end', () => {
		assert.deepEqual(pushHistory([], 'a'), ['a']);
		assert.deepEqual(pushHistory(['a', 'b'], 'a'), ['b', 'a']);
	});

	test('pushHistory ignores blank lines and respects the cap', () => {
		const history = ['a'];
		assert.deepEqual(pushHistory(history, '   '), history);
		assert.deepEqual(pushHistory(['a', 'b', 'c'], 'd', 2), ['c', 'd']);
	});

	test('Up from a fresh line selects the newest, and stops at the oldest', () => {
		assert.equal(historyBack([], null), null);
		assert.equal(historyBack(['a', 'b'], null), 1);
		assert.equal(historyBack(['a', 'b'], 1), 0);
		assert.equal(historyBack(['a', 'b'], 0), 0);
	});

	test('Down walks forward and then leaves history', () => {
		assert.equal(historyForward(['a', 'b'], null), null);
		assert.equal(historyForward(['a', 'b'], 0), 1);
		assert.equal(historyForward(['a', 'b'], 1), null);
	});
});

describe('repl errors', () => {
	test('a bare message stays bare', () => {
		assert.equal(formatReplError({ message: 'Argument ERROR' }), 'Argument ERROR');
	});

	test('file and position are prefixed when present', () => {
		const formatted = formatReplError({
			message: 'bad',
			file: 'main.fxc',
			range: { start: { line: 0, character: 2 }, end: { line: 0, character: 3 } }
		});
		assert.equal(formatted, 'main.fxc:1:3: bad');
	});

	test('a thrown value becomes a message without escaping', () => {
		assert.equal(describeThrown(new Error('boom')), 'boom');
		assert.equal(describeThrown('boom'), 'boom');
		assert.equal(describeThrown(42), '42');
	});
});
