<script lang="ts">
	/**
	 * Copy and download the `.fxc` agent skill.
	 *
	 * The skill is a single markdown file an agent can be handed on its own, so
	 * the site offers it verbatim instead of only as a rendered page. The copy
	 * control reports success and failure visibly — a silent failure would leave
	 * the reader thinking they had the file — and the download builds a Blob and
	 * revokes its object URL once the browser has read it.
	 *
	 * It lives beside the docs routes rather than in `$lib/components` so it
	 * stays with the only pages that use it.
	 */
	import Check from '@lucide/svelte/icons/check';
	import Copy from '@lucide/svelte/icons/copy';
	import Download from '@lucide/svelte/icons/download';
	import TriangleAlert from '@lucide/svelte/icons/triangle-alert';

	import { skillsMarkdown } from '$lib/docs.generated';

	/** `compact` shortens the labels for use inside a document header. */
	interface Props {
		compact?: boolean;
	}

	let { compact = false }: Props = $props();

	type Status = 'idle' | 'copied' | 'failed';
	let status = $state<Status>('idle');
	let timer: ReturnType<typeof setTimeout> | undefined;

	function flash(next: Exclude<Status, 'idle'>, delay = 2000) {
		status = next;
		clearTimeout(timer);
		timer = setTimeout(() => (status = 'idle'), delay);
	}

	async function copy() {
		try {
			await navigator.clipboard.writeText(skillsMarkdown);
			flash('copied');
		} catch {
			flash('failed', 3000);
		}
	}

	function download() {
		const blob = new Blob([skillsMarkdown], { type: 'text/markdown;charset=utf-8' });
		const url = URL.createObjectURL(blob);
		try {
			const anchor = document.createElement('a');
			anchor.href = url;
			anchor.download = 'SKILLS.md';
			anchor.click();
		} finally {
			// Revoking too soon can cancel the download before the browser reads
			// the blob, so let the click be dispatched first.
			setTimeout(() => URL.revokeObjectURL(url), 1000);
		}
	}

	const button =
		'inline-flex items-center gap-1.5 rounded-md border px-2.5 py-1.5 text-xs font-medium transition-colors';
</script>

<div class="flex flex-wrap items-center gap-2">
	<button
		type="button"
		class="{button} {status === 'failed'
			? 'border-red-500/40 text-red-400'
			: status === 'copied'
				? 'border-emerald-500/40 text-emerald-400'
				: 'border-neutral-700 text-neutral-200 hover:bg-neutral-800 hover:text-neutral-100'}"
		onclick={copy}
	>
		{#if status === 'copied'}
			<Check class="size-3.5" /> Copied to clipboard
		{:else if status === 'failed'}
			<TriangleAlert class="size-3.5" /> Copy failed — select the text instead
		{:else}
			<Copy class="size-3.5" />
			{compact ? 'Copy SKILLS.md' : 'Copy SKILLS.md for your agent'}
		{/if}
	</button>

	<button
		type="button"
		class="{button} border-neutral-700 text-neutral-300 hover:bg-neutral-800 hover:text-neutral-100"
		onclick={download}
	>
		<Download class="size-3.5" />
		Download SKILLS.md
	</button>
</div>
