<script lang="ts">
	/**
	 * The 40 scientific constants the machine carries, as a reference table.
	 *
	 * Loaded once from the module and filtered in the page, because the list is
	 * fixed and small. The value is rendered exactly as the module supplies it;
	 * a `null` value is one the calculator shows symbolically.
	 */
	import Search from '@lucide/svelte/icons/search';

	import type { PhysicalConstant } from '$lib/fx50';

	interface Props {
		constants: PhysicalConstant[];
	}

	let { constants }: Props = $props();

	let query = $state('');

	const filtered = $derived.by(() => {
		const needle = query.trim().toLowerCase();
		if (needle === '') return constants;
		return constants.filter(
			(constant) =>
				constant.name.toLowerCase().includes(needle) ||
				constant.symbol.toLowerCase().includes(needle) ||
				constant.description.toLowerCase().includes(needle)
		);
	});

	function formatValue(value: number | null): string {
		return value === null ? '—' : value.toExponential(6);
	}
</script>

<div class="flex h-full flex-col">
	<div class="border-b border-neutral-800/60 px-3 py-1.5">
		<label class="flex items-center gap-2 text-xs text-neutral-400">
			<Search class="size-3.5" />
			<input
				bind:value={query}
				placeholder="Filter constants…"
				autocomplete="off"
				spellcheck="false"
				class="w-full bg-transparent text-xs text-neutral-100 placeholder:text-neutral-600 focus:outline-none"
			/>
		</label>
	</div>

	<div class="min-h-0 flex-1 overflow-auto">
		{#if constants.length === 0}
			<p class="p-3 text-xs text-neutral-500">Loading the constants…</p>
		{:else if filtered.length === 0}
			<p class="p-3 text-xs text-neutral-500">No constant matches “{query}”.</p>
		{:else}
			<table class="w-full border-collapse text-xs">
				<thead>
					<tr class="border-b border-neutral-800/60 text-left text-neutral-500">
						<th class="px-3 py-1.5 font-medium">#</th>
						<th class="px-3 py-1.5 font-medium">Symbol</th>
						<th class="px-3 py-1.5 font-medium">Name</th>
						<th class="px-3 py-1.5 text-right font-medium">Value</th>
						<th class="px-3 py-1.5 font-medium">Unit</th>
					</tr>
				</thead>
				<tbody>
					{#each filtered as constant (constant.code)}
						<tr class="border-b border-neutral-900 hover:bg-neutral-900/50">
							<td class="px-3 py-1.5 font-mono text-neutral-500">{constant.code}</td>
							<td class="px-3 py-1.5 font-mono text-neutral-200">{constant.symbol}</td>
							<td class="px-3 py-1.5 text-neutral-300" title={constant.description}
								>{constant.name}</td
							>
							<td class="px-3 py-1.5 text-right font-mono text-neutral-300"
								>{formatValue(constant.value)}</td
							>
							<td class="px-3 py-1.5 font-mono text-neutral-500">{constant.unit}</td>
						</tr>
					{/each}
				</tbody>
			</table>
		{/if}
	</div>
</div>
