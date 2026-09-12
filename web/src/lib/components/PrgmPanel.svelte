<script lang="ts">
	/**
	 * The PRGM listing: what the transpiler emitted for the active file.
	 *
	 * The ASCII toggle re-runs the transpiler with the machine's glyphs spelled
	 * as their keyboard aliases. It is here rather than in the editor because it
	 * is a property of the listing, not of the document.
	 */
	import TriangleAlert from '@lucide/svelte/icons/triangle-alert';

	interface Props {
		prgm: string | null;
		error: string | null;
		ascii?: boolean;
		onascii?: (value: boolean) => void;
	}

	let { prgm, error, ascii = $bindable(false), onascii }: Props = $props();
</script>

<div class="flex h-full flex-col">
	<div class="flex items-center gap-3 border-b border-neutral-800/60 px-3 py-1.5">
		<label class="flex cursor-pointer items-center gap-1.5 text-xs text-neutral-400">
			<input
				type="checkbox"
				checked={ascii}
				onchange={(event) => onascii?.((event.currentTarget as HTMLInputElement).checked)}
				class="size-3.5 accent-neutral-400"
			/>
			ASCII aliases
		</label>
		{#if prgm !== null}
			<span class="ml-auto text-xs text-neutral-600">
				{prgm.split('\n').length} line{prgm.split('\n').length === 1 ? '' : 's'}
			</span>
		{/if}
	</div>

	<div class="min-h-0 flex-1 overflow-auto">
		{#if error}
			<div class="flex items-start gap-2 p-3 text-xs text-red-400">
				<TriangleAlert class="mt-0.5 size-4 shrink-0" />
				<span class="font-mono whitespace-pre-wrap">{error}</span>
			</div>
		{:else if prgm !== null}
			{#if prgm.trim() === ''}
				<p class="p-3 text-xs text-neutral-500">
					This file defines functions but no <code>fn main()</code>, so there is no program to emit.
				</p>
			{:else}
				<pre
					class="p-3 font-mono text-xs leading-relaxed whitespace-pre text-neutral-200">{prgm}</pre>
			{/if}
		{:else}
			<p class="p-3 text-xs text-neutral-500">Open a <code>.fxc</code> file to see its PRGM.</p>
		{/if}
	</div>
</div>
