/**
 * The layout a workbench remembers: which panels are open, and how the splits
 * are sized.
 *
 * This module is deliberately pure — no `localStorage`, no runes, no browser —
 * because reading storage is the part that has to be defensive, and the way to
 * make "defensive" mean something is to test it. The rune wrapper lives in
 * `layout.svelte.ts` and does nothing but hold these values and call the
 * storage functions at call time.
 *
 * The stored shape is one JSON blob under one key:
 *
 *   { "version": 1, "panels": { "run": true }, "sizes": { "main": 420 } }
 *
 * `version` is checked, not migrated: a blob from a shape this build does not
 * know is discarded rather than half-read, because a half-read layout is how a
 * panel ends up in a state no code can produce. Unknown keys, wrong types,
 * negative sizes and non-finite numbers are dropped individually, so a partly
 * valid blob still restores the parts that make sense.
 */

/** The one key the whole layout lives under. */
export const LAYOUT_STORAGE_KEY = 'fx50.layout';

/** The shape this build writes and understands. */
export const LAYOUT_VERSION = 1;

/** The persisted layout. */
export interface LayoutData {
	version: number;
	/** Panel id to open/closed. */
	panels: Record<string, boolean>;
	/** Split id to first-pane size in pixels. */
	sizes: Record<string, number>;
}

/** A fresh, empty layout. */
export function emptyLayout(): LayoutData {
	return { version: LAYOUT_VERSION, panels: {}, sizes: {} };
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/**
 * Coerce anything that came out of storage into a layout.
 *
 * A value that is not an object, or whose `version` is not this build's, is an
 * empty layout. Within the right version, a panel flag is kept only when it is
 * a boolean and a size only when it is a finite, non-negative number; the rest
 * is silently ignored, so one bad entry cannot take the others down.
 */
export function normalizeLayout(value: unknown): LayoutData {
	const result = emptyLayout();
	if (!isRecord(value)) return result;
	if (value.version !== LAYOUT_VERSION) return result;

	if (isRecord(value.panels)) {
		for (const [id, open] of Object.entries(value.panels)) {
			if (id.length > 0 && typeof open === 'boolean') result.panels[id] = open;
		}
	}

	if (isRecord(value.sizes)) {
		for (const [id, size] of Object.entries(value.sizes)) {
			if (id.length > 0 && typeof size === 'number' && Number.isFinite(size) && size >= 0) {
				result.sizes[id] = size;
			}
		}
	}

	return result;
}

/** Parse a stored blob. Absent, empty and unreadable all read as empty. */
export function parseLayout(raw: string | null | undefined): LayoutData {
	if (typeof raw !== 'string' || raw.trim() === '') return emptyLayout();
	try {
		return normalizeLayout(JSON.parse(raw));
	} catch {
		return emptyLayout();
	}
}

/** Serialise a layout, normalised first so only a readable shape is written. */
export function serializeLayout(data: LayoutData): string {
	return JSON.stringify(normalizeLayout(data));
}

/** A copy with one panel's visibility set. Pure; the input is not modified. */
export function withPanel(data: LayoutData, id: string, visible: boolean): LayoutData {
	if (id.length === 0) return data;
	return { ...data, panels: { ...data.panels, [id]: visible } };
}

/** A copy with one split's size set. A size that could not be used is ignored. */
export function withSize(data: LayoutData, id: string, size: number): LayoutData {
	if (id.length === 0 || !Number.isFinite(size) || size < 0) return data;
	return { ...data, sizes: { ...data.sizes, [id]: size } };
}
