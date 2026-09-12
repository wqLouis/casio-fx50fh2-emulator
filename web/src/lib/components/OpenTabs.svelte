<script lang="ts">
	/**
	 * The open-file tabs.
	 *
	 * Hand-rolled rather than Bits UI's tabs: a tab carries a close control and
	 * a dirty marker, and the selector has to survive a file disappearing and
	 * reappearing, which the controlled `Tabs` would need value juggling for.
	 * The ARIA shape is the same one Bits UI produces — `tablist`/`tab`, roving
	 * tabindex, arrow keys — so keyboard users get the standard behaviour.
	 */
	import X from '@lucide/svelte/icons/x';

	interface Props {
		tabs: string[];
		active: string | null;
		dirty: ReadonlySet<string>;
		onselect: (path: string) => void;
		onclose: (path: string) => void;
	}

	let { tabs, active, dirty, onselect, onclose }: Props = $props();

	let listEl: HTMLDivElement | null = $state(null);

	function label(path: string): string {
		return path.split('/').pop() ?? path;
	}

	function focusAt(index: number) {
		if (tabs.length === 0) return;
		const path = tabs[Math.max(0, Math.min(index, tabs.length - 1))];
		onselect(path);
		listEl?.querySelector<HTMLButtonElement>(`[data-tab="${CSS.escape(path)}"]`)?.focus();
	}

	function onKeyDown(event: KeyboardEvent) {
		const index = active ? tabs.indexOf(active) : -1;
		switch (event.key) {
			case 'ArrowRight':
				event.preventDefault();
				focusAt(index + 1);
				break;
			case 'ArrowLeft':
				event.preventDefault();
				focusAt(index - 1);
				break;
			case 'Home':
				event.preventDefault();
				focusAt(0);
				break;
			case 'End':
				event.preventDefault();
				focusAt(tabs.length - 1);
				break;
			case 'Delete':
			case 'Backspace':
				if (active) {
					event.preventDefault();
					onclose(active);
				}
				break;
		}
	}
</script>

<div
	bind:this={listEl}
	role="tablist"
	aria-label="Open files"
	tabindex="-1"
	class="flex shrink-0 items-stretch overflow-x-auto border-b border-neutral-800 bg-neutral-950"
	onkeydown={onKeyDown}
>
	{#each tabs as path (path)}
		<div
			class="group flex shrink-0 items-center border-r border-neutral-800 {path === active
				? 'bg-neutral-900'
				: 'bg-neutral-950 hover:bg-neutral-900/60'}"
		>
			<button
				type="button"
				role="tab"
				data-tab={path}
				aria-selected={path === active}
				title={path}
				tabindex={path === active ? 0 : -1}
				onclick={() => onselect(path)}
				class="flex items-center gap-1.5 py-1.5 pr-1 pl-3 text-xs outline-none focus-visible:ring-2 focus-visible:ring-neutral-600 {path ===
				active
					? 'text-neutral-100'
					: 'text-neutral-400'}"
			>
				{label(path)}
				{#if dirty.has(path)}
					<span class="size-1.5 rounded-full bg-amber-400" title="Unsaved changes"></span>
				{/if}
			</button>
			<button
				type="button"
				aria-label="Close {label(path)}"
				onclick={() => onclose(path)}
				class="mr-1 rounded p-0.5 text-neutral-500 opacity-0 group-hover:opacity-100 hover:bg-neutral-800 hover:text-neutral-100 focus-visible:opacity-100"
			>
				<X class="size-3" />
			</button>
		</div>
	{/each}
</div>
