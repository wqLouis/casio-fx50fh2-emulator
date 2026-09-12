/**
 * The bits of the interactive panel that are logic rather than pixels.
 *
 * Kept out of the component so they can be tested without a DOM: history
 * navigation has an off-by-one in it waiting to happen, and parsing the numbers
 * for `input()` is where a typo must become a reported error rather than a
 * silently dropped value. The component draws; this decides.
 */

import type { FxError } from '$lib/fx50';

/**
 * Append a line to a shell history, newest last.
 *
 * A repeated line moves to the end rather than being stored twice, which is
 * what makes pressing Up after re-running something give the line before it.
 * The cap keeps a long session from growing without bound.
 */
export function pushHistory(history: string[], source: string, limit = 200): string[] {
	const trimmed = source.trim();
	if (trimmed === '') return history;
	const next = history.filter((entry) => entry !== trimmed);
	next.push(trimmed);
	return next.length > limit ? next.slice(next.length - limit) : next;
}

/**
 * The index Up should select, or `null` when there is nothing to select.
 *
 * `index` is the currently selected position, and `null` means "editing a fresh
 * line". Up from a fresh line selects the newest entry; Up from the oldest
 * stays there. Down is [historyForward].
 */
export function historyBack(history: string[], index: number | null): number | null {
	if (history.length === 0) return null;
	if (index === null) return history.length - 1;
	return Math.max(0, Math.min(index, history.length - 1) - 1);
}

/**
 * The index Down should select, or `null` to return to the fresh line.
 *
 * Down from the newest entry leaves history entirely (the component restores
 * whatever draft was being typed before Up was pressed).
 */
export function historyForward(history: string[], index: number | null): number | null {
	if (index === null) return null;
	if (index >= history.length - 1) return null;
	return index + 1;
}

/**
 * Parse the values for `input()` prompts.
 *
 * Returns `null` for anything that is not a list of numbers, so the caller can
 * report it instead of passing `NaN` into the module. An empty string is an
 * empty list, not an error: a line with no `input()` needs no values.
 */
export function parseInputs(text: string): number[] | null {
	const trimmed = text.trim();
	if (trimmed === '') return [];
	const parts = trimmed.split(/[\s,]+/).filter((part) => part.length > 0);
	const values: number[] = [];
	for (const part of parts) {
		const value = Number(part);
		if (!Number.isFinite(value)) return null;
		values.push(value);
	}
	return values;
}

/** The error envelope as one line, with the file and position when it has them. */
export function formatReplError(error: FxError): string {
	const position = error.range
		? `${error.range.start.line + 1}:${error.range.start.character + 1}`
		: null;
	const where = [error.file, position].filter((part): part is string => Boolean(part)).join(':');
	return where ? `${where}: ${error.message}` : error.message;
}

/** A thrown value as a message. The wasm should not throw, but a panel must not die if it does. */
export function describeThrown(cause: unknown): string {
	return cause instanceof Error ? cause.message : String(cause);
}
