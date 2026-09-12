<script lang="ts">
	/**
	 * Diagnostics, plus the first transpile failure when there is one.
	 *
	 * Diagnostics and the transpile error are kept apart because the module can
	 * report both: a document can have markers and still fail to build. The
	 * interesting diagnostic is the first, so it is not collapsed behind a
	 * disclosure.
	 */
	import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
	import { DiagnosticSeverity, type Diagnostic } from 'vscode-languageserver-types';

	import { formatRange } from './format';

	interface Props {
		diagnostics: Diagnostic[];
		compileError: string | null;
	}

	let { diagnostics, compileError }: Props = $props();

	const SEVERITY: Record<number, { label: string; className: string }> = {
		[DiagnosticSeverity.Error]: { label: 'error', className: 'text-red-400' },
		[DiagnosticSeverity.Warning]: { label: 'warning', className: 'text-amber-400' },
		[DiagnosticSeverity.Information]: { label: 'info', className: 'text-sky-400' },
		[DiagnosticSeverity.Hint]: { label: 'hint', className: 'text-neutral-400' }
	};

	function severityOf(diagnostic: Diagnostic) {
		return SEVERITY[diagnostic.severity ?? DiagnosticSeverity.Error] ?? SEVERITY[1];
	}
</script>

<div class="h-full overflow-auto p-3">
	{#if compileError}
		<div
			class="mb-3 flex items-start gap-2 rounded-md border border-red-900/50 bg-red-950/30 p-3 text-xs text-red-300"
		>
			<TriangleAlert class="mt-0.5 size-4 shrink-0" />
			<span class="font-mono whitespace-pre-wrap">{compileError}</span>
		</div>
	{/if}

	{#if diagnostics.length === 0}
		{#if !compileError}
			<p class="text-xs text-neutral-500">No problems.</p>
		{/if}
	{:else}
		<ul class="space-y-1">
			{#each diagnostics as diagnostic, index (index)}
				{@const severity = severityOf(diagnostic)}
				<li class="rounded-md border border-neutral-800/60 bg-neutral-950 px-3 py-2 text-xs">
					<div class="flex items-start gap-2">
						<span class="mt-px font-mono text-[10px] tracking-wide uppercase {severity.className}"
							>{severity.label}</span
						>
						<span class="flex-1 whitespace-pre-wrap text-neutral-200">{diagnostic.message}</span>
						{#if formatRange(diagnostic.range)}
							<span class="shrink-0 font-mono text-[10px] text-neutral-500"
								>{formatRange(diagnostic.range)}</span
							>
						{/if}
					</div>
				</li>
			{/each}
		</ul>
	{/if}
</div>
