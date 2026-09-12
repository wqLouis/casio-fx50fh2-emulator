<script lang="ts">
	/**
	 * The program's embedded `#tests`, run through the wasm module.
	 *
	 * A report with failures is a normal result, not an error, so it is rendered
	 * as a list with the failing cases expanded; `error` is only for a suite the
	 * module could not run at all.
	 */
	import FlaskConical from '@lucide/svelte/icons/flask-conical';
	import CircleCheck from '@lucide/svelte/icons/circle-check';
	import CircleX from '@lucide/svelte/icons/circle-x';
	import TriangleAlert from '@lucide/svelte/icons/triangle-alert';

	import type { TestReport } from '$lib/fx50';

	interface Props {
		report: TestReport | null;
		error: string | null;
		disabled?: boolean;
		ontest: () => void;
	}

	let { report, error, disabled = false, ontest }: Props = $props();
</script>

<div class="h-full overflow-auto p-3">
	<div class="flex items-center gap-3">
		<button
			type="button"
			onclick={ontest}
			{disabled}
			class="inline-flex items-center gap-1.5 rounded-md border border-neutral-700 px-3 py-1.5 text-sm font-medium text-neutral-200 transition-colors hover:bg-neutral-800 disabled:cursor-not-allowed disabled:text-neutral-600"
		>
			<FlaskConical class="size-4" />
			Run tests
		</button>
		{#if report}
			<span
				class="text-xs"
				class:text-emerald-400={report.success}
				class:text-red-400={!report.success}
			>
				{report.passed} passed, {report.failed} failed
			</span>
		{/if}
	</div>

	{#if error}
		<div class="mt-4 flex items-start gap-2 text-xs text-red-400">
			<TriangleAlert class="mt-0.5 size-4 shrink-0" />
			<span class="font-mono whitespace-pre-wrap">{error}</span>
		</div>
	{:else if report}
		{#if report.cases.length === 0}
			<p class="mt-4 text-xs text-neutral-500">This program carries no test cases.</p>
		{:else}
			<ul class="mt-3 space-y-1">
				{#each report.cases as testCase (testCase.name)}
					<li
						class="rounded-md border px-3 py-2 text-xs {testCase.passed
							? 'border-neutral-800/60 bg-neutral-950'
							: 'border-red-900/50 bg-red-950/30'}"
					>
						<div class="flex items-start gap-2">
							{#if testCase.passed}
								<CircleCheck class="mt-0.5 size-4 shrink-0 text-emerald-500" />
							{:else}
								<CircleX class="mt-0.5 size-4 shrink-0 text-red-500" />
							{/if}
							<span class="font-medium text-neutral-200">{testCase.name}</span>
						</div>
						{#if !testCase.passed}
							<dl class="mt-2 grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 pl-6">
								<dt class="text-neutral-500">Expected</dt>
								<dd class="font-mono text-neutral-300">{testCase.expected}</dd>
								<dt class="text-neutral-500">Actual</dt>
								<dd class="font-mono text-neutral-300">{testCase.actual}</dd>
							</dl>
						{/if}
					</li>
				{/each}
			</ul>
		{/if}
	{:else}
		<p class="mt-4 text-xs text-neutral-500">
			Run the program's <code>#tests</code> table, if it has one.
		</p>
	{/if}
</div>
