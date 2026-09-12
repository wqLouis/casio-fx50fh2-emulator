/**
 * The arithmetic of a two-pane split.
 *
 * A split is a size for the *first* pane along one axis; the second pane takes
 * whatever is left, so it can never be separated from the first by construction.
 * That makes the whole problem "clamp a proposed first-pane size", which is the
 * part that goes subtly wrong: a drag past the far end, a window narrower than
 * the two minimums, a stored size from a larger monitor. Everything here is
 * pure and unit-tested, so the component only has to move numbers around.
 *
 * Sizes are pixels along the split axis — width for a horizontal split, height
 * for a vertical one. A caller that persists them gets a size that is clamped
 * again on restore, which is what makes a saved arrangement survive a resize.
 */

/** A proposed first-pane size and the limits it must respect. */
export interface SplitInput {
	/** Proposed size of the first pane, in pixels. */
	size: number;
	/** Minimum size of the first pane, in pixels. */
	min: number;
	/** Minimum size of the second pane, in pixels. */
	minOther: number;
	/** Available size along the split axis, in pixels. */
	total: number;
}

/** The inclusive range the first pane may occupy, for ARIA and keyboard steps. */
export interface SplitBounds {
	min: number;
	max: number;
}

/** A number that is actually usable, falling back when it is not finite. */
function finite(value: number, fallback = 0): number {
	return Number.isFinite(value) ? value : fallback;
}

/**
 * The range the first pane may occupy.
 *
 * The normal case is `[min, total - minOther]`. When the container is too small
 * to give both panes their minimum — a narrow window, a first paint before
 * measurement, or a minimum larger than the container — there is no valid range,
 * so instead of returning an inverted one (`min > max`, which a naive clamp
 * turns into a negative or overflowing size) both bounds collapse onto the point
 * that keeps the two minimums' ratio. That point is always inside `[0, total]`,
 * so the result can never be negative or larger than the container.
 */
export function splitBounds(min: number, minOther: number, total: number): SplitBounds {
	const axis = Math.max(0, finite(total));
	const lower = Math.max(0, finite(min));
	const upper = Math.max(0, finite(minOther));

	if (axis === 0) return { min: 0, max: 0 };

	const lowest = Math.min(lower, axis);
	const highest = Math.max(0, axis - upper);
	if (lowest <= highest) return { min: lowest, max: highest };

	// No room for both minimums. Meet in the middle of the impossible range,
	// weighted by the minimums so a pane asking for more gets proportionally
	// more.
	const sum = lower + upper;
	const ratio = sum > 0 ? lower / sum : 0.5;
	const meet = axis * ratio;
	return { min: meet, max: meet };
}

/**
 * Clamp a proposed first-pane size against both minimums and the container.
 *
 * The result is always finite and lies in `[0, total]`. A non-finite proposal
 * (a division that went wrong upstream) reads as `0` and is then clamped up to
 * the lower bound.
 */
export function clampPaneSize({ size, min, minOther, total }: SplitInput): number {
	const proposed = finite(size);
	const bounds = splitBounds(min, minOther, total);
	if (bounds.min === bounds.max) return bounds.min;
	return Math.min(Math.max(proposed, bounds.min), bounds.max);
}

/** A ratio reduced to the `[0, 1]` a `flex-basis` percentage can use. */
export function clampRatio(ratio: number): number {
	if (!Number.isFinite(ratio)) return 0.5;
	return Math.min(1, Math.max(0, ratio));
}

/** The sensible starting size: a fraction of the container, then clamped. */
export function defaultPaneSize(total: number, min: number, minOther: number, ratio = 0.5): number {
	const axis = Math.max(0, finite(total));
	return clampPaneSize({ size: axis * clampRatio(ratio), min, minOther, total: axis });
}
