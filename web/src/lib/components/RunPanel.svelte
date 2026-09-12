<script lang="ts">
	/**
	 * Running the program: the `?` inputs, the `◢` displays, and the machine's
	 * registers afterwards.
	 *
	 * The number of prompts is a property of the emitted PRGM — one `?` per read —
	 * so the input fields are derived from it rather than configured. Values are
	 * kept as text and parsed on Run, because `?` reads a real number and a value
	 * that does not parse should be reported, not silently become `NaN`.
	 */
	import Play from '@lucide/svelte/icons/play';
	import TriangleAlert from '@lucide/svelte/icons/triangle-alert';

	import type { RunResult } from '$lib/fx50';

	interface Props {
		/** How many `?` prompts the transpiled program has. */
		prompts: number;
		result: RunResult | null;
		error: string | null;
		disabled?: boolean;
		onrun: (inputs: number[]) => void;
	}

	let { prompts, result, error, disabled = false, onrun }: Props = $props();

	let raw = $state<string[]>([]);
	let inputError = $state<string | null>(null);

	// Grow and shrink the fields with the program's prompt count without
	// discarding what the user already typed (a re-transpile should not clear
	// the inputs).
	$effect(() => {
		if (raw.length !== prompts) {
			raw = Array.from({ length: prompts }, (_, index) => raw[index] ?? '');
		}
	});

	function run() {
		const inputs: number[] = [];
		for (const [index, text] of raw.entries()) {
			const value = text.trim() === '' ? Number.NaN : Number(text);
			if (Number.isNaN(value)) {
				inputError = `Input ${index + 1} is not a number.`;
				return;
			}
			inputs.push(value);
		}
		inputError = null;
		onrun(inputs);
	}

	const memoryEntries = $derived(Object.entries(result?.state.memories ?? {}));
</script>

<div class="h-full overflow-auto p-3">
	<div class="flex items-center gap-2">
		<button
			type="button"
			onclick={run}
			{disabled}
			class="inline-flex items-center gap-1.5 rounded-md bg-emerald-600 px-3 py-1.5 text-sm font-medium text-white transition-colors hover:bg-emerald-500 disabled:cursor-not-allowed disabled:bg-neutral-800 disabled:text-neutral-500"
		>
			<Play class="size-4" />
			Run
		</button>
		<span class="text-xs text-neutral-500">
			{prompts === 0 ? 'This program reads no input.' : `${prompts} input(s) expected.`}
		</span>
	</div>

	{#if prompts > 0}
		<div class="mt-3 grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
			{#each raw as _, index (index)}
				<label class="flex items-center gap-2 text-xs text-neutral-400">
					<span class="w-12 shrink-0 font-mono">? {index + 1}</span>
					<input
						type="text"
						inputmode="decimal"
						bind:value={raw[index]}
						class="w-full rounded-md border border-neutral-700 bg-neutral-950 px-2 py-1 font-mono text-xs text-neutral-100 focus:border-neutral-500 focus:outline-none"
					/>
				</label>
			{/each}
		</div>
	{/if}

	{#if inputError}
		<p class="mt-2 text-xs text-red-400">{inputError}</p>
	{/if}

	{#if error}
		<div class="mt-4 flex items-start gap-2 text-xs text-red-400">
			<TriangleAlert class="mt-0.5 size-4 shrink-0" />
			<span class="font-mono whitespace-pre-wrap">{error}</span>
		</div>
	{:else if result}
		<div class="mt-4">
			<h3 class="text-[10px] font-semibold tracking-wide text-neutral-500 uppercase">Displays</h3>
			{#if result.outputs.length === 0}
				<p class="mt-1 text-xs text-neutral-500">Nothing was displayed.</p>
			{:else}
				<ol class="mt-1 space-y-0.5 font-mono text-sm">
					{#each result.outputs as output, index (index)}
						<li>{output}</li>
					{/each}
				</ol>
			{/if}
		</div>

		<div class="mt-4">
			<h3 class="text-[10px] font-semibold tracking-wide text-neutral-500 uppercase">
				Machine state
			</h3>
			<dl class="mt-1 grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-xs">
				<dt class="text-neutral-500">Ans</dt>
				<dd class="font-mono text-neutral-200">{result.state.ans.display}</dd>
				<dt class="text-neutral-500">Display</dt>
				<dd class="font-mono text-neutral-200">{result.state.display}</dd>
				<dt class="text-neutral-500">Mode</dt>
				<dd class="font-mono text-neutral-200">{result.state.mode}</dd>
				<dt class="text-neutral-500">Angle</dt>
				<dd class="font-mono text-neutral-200">{result.state.angle}</dd>
				{#if result.state.base}
					<dt class="text-neutral-500">Base</dt>
					<dd class="font-mono text-neutral-200">{result.state.base}</dd>
				{/if}
			</dl>
		</div>

		{#if memoryEntries.length > 0}
			<div class="mt-4">
				<h3 class="text-[10px] font-semibold tracking-wide text-neutral-500 uppercase">Memories</h3>
				<dl class="mt-1 grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-xs">
					{#each memoryEntries as [name, value] (name)}
						<dt class="font-mono text-neutral-500">{name}</dt>
						<dd class="font-mono text-neutral-200">{value.display}</dd>
					{/each}
				</dl>
			</div>
		{/if}
	{:else}
		<p class="mt-4 text-xs text-neutral-500">Run the program to see its output.</p>
	{/if}
</div>
