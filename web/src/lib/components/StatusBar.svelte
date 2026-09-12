<script lang="ts">
	/**
	 * The status bar: what the module is, what is open, the machine's limits, and
	 * a count of anything wrong.
	 */
	import TriangleAlert from '@lucide/svelte/icons/triangle-alert';

	import type { VersionInfo } from '$lib/fx50';
	import type { WorkspaceKind } from '$lib/workspace/types';

	interface Props {
		version: string | null;
		workspaceName: string;
		kind: WorkspaceKind | null;
		limits: VersionInfo['limits'] | null;
		problemCount: number;
		error: string | null;
	}

	let { version, workspaceName, kind, limits, problemCount, error }: Props = $props();
</script>

<footer
	class="flex shrink-0 items-center gap-3 border-t border-neutral-800 bg-neutral-950 px-3 py-1 text-[11px] text-neutral-500"
>
	<span class="font-mono">{version ? `fx50 ${version}` : 'fx50 —'}</span>
	<span class="text-neutral-700">|</span>
	<span>
		{workspaceName || 'no workspace'}{kind ? ` (${kind})` : ''}
	</span>
	{#if limits}
		<span class="text-neutral-700">|</span>
		<span>
			{limits.programKeys} bytes · {limits.memories} memories · {limits.constants} constants
		</span>
	{/if}
	<span class="ml-auto flex items-center gap-1.5">
		{#if error}
			<TriangleAlert class="size-3 text-red-400" />
			<span class="max-w-72 truncate text-red-400" title={error}>{error}</span>
		{:else if problemCount > 0}
			<TriangleAlert class="size-3 text-amber-400" />
			<span class="text-amber-400">{problemCount} problem{problemCount === 1 ? '' : 's'}</span>
		{:else}
			<span>no problems</span>
		{/if}
	</span>
</footer>
