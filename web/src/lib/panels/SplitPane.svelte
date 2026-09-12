<script lang="ts">
	/**
	 * Two panes with a draggable divider, in the shape VS Code uses.
	 *
	 * The component owns the pointer and keyboard handling; the size arithmetic
	 * is in `split.ts` so the fragile part — never letting a pane collapse to
	 * zero or a drag run past the end — is unit-tested. The size is emitted so
	 * the shell can persist it per split and restore the arrangement.
	 *
	 * Sizes are pixels along the axis. A restored size is clamped against the
	 * current container on every render, so opening the page in a narrower
	 * window cannot produce a pane wider than the window.
	 */
	import type { Snippet } from 'svelte';

	import { clampPaneSize, clampRatio, defaultPaneSize, splitBounds } from './split';
	import type { SplitOrientation } from './types';

	interface Props {
		/** Layout direction. `horizontal` puts the panes side by side. */
		orientation?: SplitOrientation;
		/** Minimum size of the first pane, in pixels. */
		min?: number;
		/** Minimum size of the second pane, in pixels. */
		minOther?: number;
		/** Fraction of the container the first pane takes before any drag. */
		defaultRatio?: number;
		/** First-pane size in pixels. `null` means "use the default split". */
		size?: number | null;
		/** Called with the first-pane size whenever the user changes it. */
		onsize?: (size: number) => void;
		/** Accessible name for the separator. */
		label?: string;
		first: Snippet;
		second: Snippet;
	}

	let {
		orientation = 'horizontal',
		min = 120,
		minOther = 120,
		defaultRatio = 0.5,
		size = $bindable<number | null>(null),
		onsize,
		label = 'Resize panels',
		first,
		second
	}: Props = $props();

	/** Keyboard step, and the larger step Shift selects. */
	const STEP = 16;
	const LARGE_STEP = 64;

	let containerEl: HTMLDivElement | null = $state(null);
	let dividerEl: HTMLDivElement | null = $state(null);
	let width = $state(0);
	let height = $state(0);
	let dragging = $state(false);

	const isHorizontal = $derived(orientation === 'horizontal');
	const total = $derived(isHorizontal ? width : height);
	const bounds = $derived(splitBounds(min, minOther, total));
	const effective = $derived(
		total > 0
			? clampPaneSize({
					size: size ?? total * clampRatio(defaultRatio),
					min,
					minOther,
					total
				})
			: 0
	);
	// Before the container has been measured, fall back to a percentage so the
	// first paint is the intended split rather than a collapsed pane.
	const basis = $derived(total > 0 ? `${effective}px` : `${clampRatio(defaultRatio) * 100}%`);

	/** Apply a proposed size, clamped, and tell the parent. */
	function commit(proposed: number) {
		if (total <= 0) return;
		const next = clampPaneSize({ size: proposed, min, minOther, total });
		// Do not emit on every pointer event when the clamped value has not moved.
		if (size !== null && Math.abs(size - next) < 0.5) return;
		size = next;
		onsize?.(next);
	}

	function startDrag(event: PointerEvent) {
		if (event.pointerType === 'mouse' && event.button !== 0) return;
		event.preventDefault();
		dragging = true;
		dividerEl?.setPointerCapture(event.pointerId);
	}

	function onDrag(event: PointerEvent) {
		if (!dragging || !containerEl) return;
		const rect = containerEl.getBoundingClientRect();
		const position = isHorizontal ? event.clientX : event.clientY;
		const origin = isHorizontal ? rect.left : rect.top;
		commit(position - origin);
	}

	function endDrag(event: PointerEvent) {
		if (!dragging) return;
		dragging = false;
		if (dividerEl?.hasPointerCapture(event.pointerId)) {
			dividerEl.releasePointerCapture(event.pointerId);
		}
	}

	function reset() {
		commit(defaultPaneSize(total, min, minOther, defaultRatio));
	}

	function onKeyDown(event: KeyboardEvent) {
		const step = event.shiftKey ? LARGE_STEP : STEP;
		let delta = 0;
		if (isHorizontal) {
			if (event.key === 'ArrowLeft') delta = -step;
			else if (event.key === 'ArrowRight') delta = step;
		} else {
			if (event.key === 'ArrowUp') delta = -step;
			else if (event.key === 'ArrowDown') delta = step;
		}

		if (delta !== 0) {
			event.preventDefault();
			commit(effective + delta);
			return;
		}
		if (event.key === 'Home') {
			event.preventDefault();
			commit(bounds.min);
		} else if (event.key === 'End') {
			event.preventDefault();
			commit(bounds.max);
		}
	}
</script>

<div
	bind:this={containerEl}
	bind:clientWidth={width}
	bind:clientHeight={height}
	class="flex h-full w-full overflow-hidden {isHorizontal ? 'flex-row' : 'flex-col'} {dragging
		? 'select-none'
		: ''}"
>
	<div class="relative min-h-0 min-w-0 overflow-hidden" style="flex: 0 0 {basis};">
		{@render first()}
	</div>

	<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
	<div
		bind:this={dividerEl}
		role="separator"
		tabindex="0"
		aria-label={label}
		aria-orientation={isHorizontal ? 'vertical' : 'horizontal'}
		aria-valuenow={Math.round(effective)}
		aria-valuemin={Math.round(bounds.min)}
		aria-valuemax={Math.round(bounds.max)}
		onpointerdown={startDrag}
		onpointermove={onDrag}
		onpointerup={endDrag}
		onpointercancel={endDrag}
		ondblclick={reset}
		onkeydown={onKeyDown}
		class="group relative z-10 shrink-0 outline-none focus-visible:bg-neutral-500 {isHorizontal
			? 'w-1 cursor-col-resize'
			: 'h-1 cursor-row-resize'} {dragging ? 'bg-neutral-500' : ''}"
	>
		<span
			aria-hidden="true"
			class="absolute bg-neutral-800 transition-colors group-hover:bg-neutral-600 {isHorizontal
				? 'inset-y-0 left-1/2 w-px -translate-x-1/2'
				: 'inset-x-0 top-1/2 h-px -translate-y-1/2'}"
		></span>
	</div>

	<div class="relative min-h-0 min-w-0 flex-1 overflow-hidden">
		{@render second()}
	</div>
</div>
