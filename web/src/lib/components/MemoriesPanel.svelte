<script lang="ts">
	/**
	 * The seven memories and who holds them.
	 *
	 * A memory can be held by more than one name over the life of a program,
	 * because `free` hands it back and a later `let` takes it again. The holders
	 * list is that history, and it is rendered in order: a memory whose holders
	 * read `first → second` was `first`'s until it was freed, then `second`'s.
	 */
	import type { MemoryPlan } from '$lib/fx50';

	interface Props {
		regs: MemoryPlan | null;
	}

	let { regs }: Props = $props();

	/** The machine's memory letters, in the order the calculator labels them. */
	const LETTERS = ['A', 'B', 'C', 'D', 'X', 'Y', 'M'] as const;

	const holdersByMemory = $derived(
		new Map((regs?.memories ?? []).map((entry) => [entry.memory, entry.holders]))
	);
</script>

<div class="h-full overflow-auto p-3">
	{#if !regs}
		<p class="text-xs text-neutral-500">Transpile a program to see its memory plan.</p>
	{:else}
		<ul class="grid grid-cols-2 gap-2 sm:grid-cols-3 lg:grid-cols-4">
			{#each LETTERS as letter (letter)}
				{@const holders = holdersByMemory.get(letter) ?? []}
				<li
					class="rounded-md border px-2 py-1.5 {holders.length > 0
						? 'border-neutral-700 bg-neutral-900'
						: 'border-neutral-800/60 bg-neutral-950'}"
				>
					<div class="flex items-center justify-between">
						<span class="font-mono text-sm font-semibold">{letter}</span>
						<span class="text-[10px] tracking-wide text-neutral-500 uppercase">
							{holders.length > 0 ? 'in use' : 'free'}
						</span>
					</div>
					{#if holders.length > 0}
						<div class="mt-1 flex flex-wrap items-center gap-1">
							{#each holders as holder, index (holder + index)}
								{#if index > 0}
									<span class="text-neutral-600">→</span>
								{/if}
								<span class="rounded bg-neutral-800 px-1.5 py-0.5 font-mono text-[11px]"
									>{holder}</span
								>
							{/each}
						</div>
					{:else}
						<p class="mt-1 text-[11px] text-neutral-600">unused</p>
					{/if}
				</li>
			{/each}
		</ul>

		<p class="mt-4 text-xs text-neutral-500">
			{regs.used} of {LETTERS.length} memories used{regs.free.length > 0
				? `; free: ${regs.free.join(' ')}`
				: ''}
		</p>

		<dl class="mt-3 grid grid-cols-[auto_1fr] gap-x-3 gap-y-1.5 text-xs">
			{#if regs.consts.length > 0}
				<dt class="text-neutral-500">Constants</dt>
				<dd class="flex flex-wrap gap-1">
					{#each regs.consts as name (name)}
						<span
							class="rounded bg-neutral-800 px-1.5 py-0.5 font-mono text-[11px] text-neutral-300"
							>{name}</span
						>
					{/each}
				</dd>
			{/if}
			{#if regs.data.length > 0}
				<dt class="text-neutral-500">Data tables</dt>
				<dd class="flex flex-wrap gap-1">
					{#each regs.data as name (name)}
						<span
							class="rounded bg-neutral-800 px-1.5 py-0.5 font-mono text-[11px] text-neutral-300"
							>{name}</span
						>
					{/each}
				</dd>
			{/if}
			{#if regs.freed.length > 0}
				<dt class="text-neutral-500">Released</dt>
				<dd class="flex flex-wrap gap-1">
					{#each regs.freed as name (name)}
						<span
							class="rounded bg-neutral-800/60 px-1.5 py-0.5 font-mono text-[11px] text-neutral-400"
							>{name}</span
						>
					{/each}
				</dd>
			{/if}
		</dl>
	{/if}
</div>
