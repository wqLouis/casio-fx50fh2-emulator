<script lang="ts">
	/**
	 * How many of the machine's 680 bytes the program costs, and what the
	 * optimiser saved doing it.
	 */
	import type { SizeInfo } from '$lib/fx50';

	interface Props {
		size: SizeInfo | null;
		/** Whether the optimiser ran. When it did not, the numbers describe the raw translation. */
		optimize?: boolean;
	}

	let { size, optimize = true }: Props = $props();

	const percent = $derived(
		size && size.capacity > 0 ? Math.min(100, (size.keys / size.capacity) * 100) : 0
	);
</script>

<div class="h-full overflow-auto p-3">
	{#if !size}
		<p class="text-xs text-neutral-500">Transpile a program to measure it.</p>
	{:else}
		<div class="flex items-baseline justify-between">
			<span class="text-sm font-medium">
				{size.keys} / {size.capacity} bytes
			</span>
			<span class="text-xs" class:text-red-400={!size.fits} class:text-neutral-500={size.fits}>
				{size.fits ? `${size.remaining ?? 0} left` : 'does not fit'}
			</span>
		</div>

		<div
			class="mt-2 h-3 overflow-hidden rounded-full border border-neutral-800 bg-neutral-950"
			role="progressbar"
			aria-valuemin={0}
			aria-valuemax={size.capacity}
			aria-valuenow={size.keys}
			aria-label="Program storage used"
		>
			<div
				class="h-full rounded-full transition-[width] {size.fits ? 'bg-emerald-500' : 'bg-red-500'}"
				style="width: {percent}%"
			></div>
		</div>

		<dl class="mt-4 grid grid-cols-2 gap-x-4 gap-y-2 text-xs">
			<div class="flex justify-between">
				<dt class="text-neutral-500">Statements</dt>
				<dd class="font-mono text-neutral-200">{size.statements}</dd>
			</div>
			<div class="flex justify-between">
				<dt class="text-neutral-500">Largest</dt>
				<dd class="font-mono text-neutral-200">{size.largest} keys</dd>
			</div>
			{#if size.unoptimizedKeys !== undefined}
				<div class="flex justify-between">
					<dt class="text-neutral-500">Without optimiser</dt>
					<dd class="font-mono text-neutral-200">{size.unoptimizedKeys} keys</dd>
				</div>
			{/if}
			{#if !optimize}
				<!-- Said plainly, because the numbers above are now the raw translation:
				     a reader who assumes they are optimised will draw the wrong conclusion
				     about whether their program fits. -->
				<p class="col-span-2 mt-1 text-xs text-amber-400">
					Optimiser off — these are the unoptimised figures.
				</p>
			{/if}
			{#if size.savedKeys !== undefined}
				<div class="flex justify-between">
					<dt class="text-neutral-500">Optimiser saved</dt>
					<dd class="font-mono text-emerald-400">{size.savedKeys} keys</dd>
				</div>
			{/if}
		</dl>
	{/if}
</div>
