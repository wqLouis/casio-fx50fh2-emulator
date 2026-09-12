<script lang="ts">
	/**
	 * The workspace tree.
	 *
	 * The rows are a flat list rather than nested markup: it keeps the roving
	 * tabindex and arrow-key movement simple, and `aria-level` records the depth
	 * that nesting would otherwise imply. Each row is a context menu (right
	 * click) and a dropdown (the kebab, for keyboards and touch); both offer the
	 * same create/rename/delete actions.
	 *
	 * This component owns the dialogs, not the file operations: the page gets
	 * `oncreate`/`onrename`/`ondelete` and does the workspace work, so the tree
	 * never has to know which backend is open.
	 */
	import { ContextMenu, DropdownMenu } from 'bits-ui';
	import ChevronDown from '@lucide/svelte/icons/chevron-down';
	import ChevronRight from '@lucide/svelte/icons/chevron-right';
	import Ellipsis from '@lucide/svelte/icons/ellipsis';
	import FileCode from '@lucide/svelte/icons/file-code';
	import FilePlus from '@lucide/svelte/icons/file-plus';
	import Folder from '@lucide/svelte/icons/folder';
	import FolderOpen from '@lucide/svelte/icons/folder-open';
	import Pencil from '@lucide/svelte/icons/pencil';
	import Trash2 from '@lucide/svelte/icons/trash-2';

	import type { WorkspaceEntry } from '$lib/workspace/types';

	import ConfirmDialog from './ConfirmDialog.svelte';
	import PromptDialog from './PromptDialog.svelte';

	interface Props {
		entries: WorkspaceEntry[];
		activePath: string | null;
		dirty: ReadonlySet<string>;
		onopen: (path: string) => void;
		oncreate: (path: string) => void;
		onrename: (from: string, to: string) => void;
		ondelete: (entry: WorkspaceEntry) => void;
	}

	let { entries, activePath, dirty, onopen, oncreate, onrename, ondelete }: Props = $props();

	interface FlatNode {
		entry: WorkspaceEntry;
		depth: number;
		parent: string | null;
	}

	let expanded = $state<Set<string>>(new Set());
	let focusedPath = $state<string | null>(null);
	let treeEl: HTMLDivElement | null = $state(null);

	// Dialogs. The create dialog is shared by the root button and every
	// directory's menu item; the directory it targets is kept separately.
	let createOpen = $state(false);
	let createDir = $state('');
	let renameOpen = $state(false);
	let renameEntry = $state<WorkspaceEntry | null>(null);
	let deleteOpen = $state(false);
	let deleteEntry = $state<WorkspaceEntry | null>(null);

	const item =
		'flex cursor-pointer items-center gap-2 rounded px-2 py-1.5 text-xs text-neutral-200 outline-none select-none data-[highlighted]:bg-neutral-800';

	const flat = $derived.by(() => {
		const out: FlatNode[] = [];
		const walk = (list: WorkspaceEntry[], depth: number, parent: string | null) => {
			for (const entry of list) {
				out.push({ entry, depth, parent });
				if (entry.kind === 'directory' && expanded.has(entry.path)) {
					walk(entry.children ?? [], depth + 1, entry.path);
				}
			}
		};
		walk(entries, 0, null);
		return out;
	});

	// One row has `tabindex=0` so the tree itself is reachable with Tab; the
	// arrows move within it from there.
	const tabbable = $derived(
		focusedPath ??
			(activePath && flat.some((node) => node.entry.path === activePath) ? activePath : null) ??
			flat[0]?.entry.path ??
			null
	);

	function buttonFor(path: string): HTMLButtonElement | null {
		return treeEl?.querySelector<HTMLButtonElement>(`[data-path="${CSS.escape(path)}"]`) ?? null;
	}

	function focusAt(index: number) {
		if (flat.length === 0) return;
		const node = flat[Math.max(0, Math.min(index, flat.length - 1))];
		focusedPath = node.entry.path;
		buttonFor(node.entry.path)?.focus();
	}

	function toggle(entry: WorkspaceEntry) {
		const next = new Set(expanded);
		if (next.has(entry.path)) next.delete(entry.path);
		else next.add(entry.path);
		expanded = next;
	}

	function activate(entry: WorkspaceEntry) {
		if (entry.kind === 'directory') toggle(entry);
		else onopen(entry.path);
	}

	function onKeyDown(event: KeyboardEvent) {
		const node =
			flat.find((candidate) => candidate.entry.path === focusedPath) ??
			(flat.length > 0 ? flat[0] : null);
		if (!node) return;
		const index = flat.indexOf(node);

		switch (event.key) {
			case 'ArrowDown':
				event.preventDefault();
				focusAt(index + 1);
				break;
			case 'ArrowUp':
				event.preventDefault();
				focusAt(index - 1);
				break;
			case 'ArrowRight':
				if (node.entry.kind === 'directory') {
					event.preventDefault();
					if (!expanded.has(node.entry.path)) toggle(node.entry);
					else focusAt(index + 1);
				}
				break;
			case 'ArrowLeft':
				event.preventDefault();
				if (node.entry.kind === 'directory' && expanded.has(node.entry.path)) {
					toggle(node.entry);
				} else if (node.parent) {
					focusAt(flat.findIndex((candidate) => candidate.entry.path === node.parent));
				}
				break;
			case 'Home':
				event.preventDefault();
				focusAt(0);
				break;
			case 'End':
				event.preventDefault();
				focusAt(flat.length - 1);
				break;
			case 'Enter':
			case ' ':
				event.preventDefault();
				activate(node.entry);
				break;
		}
	}

	function requestCreate(dir: string) {
		createDir = dir;
		createOpen = true;
	}

	function requestRename(entry: WorkspaceEntry) {
		renameEntry = entry;
		renameOpen = true;
	}

	function requestDelete(entry: WorkspaceEntry) {
		deleteEntry = entry;
		deleteOpen = true;
	}
</script>

<div class="flex h-full flex-col">
	<div class="flex items-center justify-between border-b border-neutral-800 px-2 py-1.5">
		<span class="text-[10px] font-semibold tracking-wide text-neutral-500 uppercase">Files</span>
		<button
			type="button"
			onclick={() => requestCreate('')}
			title="New file"
			aria-label="New file"
			class="rounded p-1 text-neutral-400 hover:bg-neutral-800 hover:text-neutral-100"
		>
			<FilePlus class="size-3.5" />
		</button>
	</div>

	<div
		bind:this={treeEl}
		role="tree"
		aria-label="Workspace files"
		tabindex="-1"
		class="min-h-0 flex-1 overflow-auto py-1"
		onkeydown={onKeyDown}
	>
		{#if flat.length === 0}
			<p class="px-3 py-2 text-xs text-neutral-600">No files yet. Create one to begin.</p>
		{/if}

		{#each flat as node (node.entry.path)}
			<ContextMenu.Root>
				<ContextMenu.Trigger class="group flex items-center pr-1">
					<button
						type="button"
						role="treeitem"
						data-path={node.entry.path}
						aria-level={node.depth + 1}
						aria-selected={node.entry.path === activePath}
						aria-expanded={node.entry.kind === 'directory'
							? expanded.has(node.entry.path)
							: undefined}
						tabindex={node.entry.path === tabbable ? 0 : -1}
						onfocus={() => (focusedPath = node.entry.path)}
						onclick={() => activate(node.entry)}
						style="padding-left: {node.depth * 12 + 4}px"
						class="flex min-w-0 flex-1 items-center gap-1.5 rounded py-1 pr-1 text-left text-xs outline-none focus-visible:ring-2 focus-visible:ring-neutral-600 {node
							.entry.path === activePath
							? 'bg-neutral-800 text-neutral-100'
							: 'text-neutral-300 hover:bg-neutral-900'}"
					>
						{#if node.entry.kind === 'directory'}
							{#if expanded.has(node.entry.path)}
								<ChevronDown class="size-3 shrink-0 text-neutral-500" />
								<FolderOpen class="size-3.5 shrink-0 text-sky-400" />
							{:else}
								<ChevronRight class="size-3 shrink-0 text-neutral-500" />
								<Folder class="size-3.5 shrink-0 text-sky-400" />
							{/if}
						{:else}
							<span class="size-3 shrink-0"></span>
							<FileCode class="size-3.5 shrink-0 text-neutral-500" />
						{/if}
						<span class="truncate">{node.entry.name}</span>
						{#if dirty.has(node.entry.path)}
							<span class="ml-auto size-1.5 shrink-0 rounded-full bg-amber-400" title="Unsaved"
							></span>
						{/if}
					</button>

					<DropdownMenu.Root>
						<DropdownMenu.Trigger
							class="rounded p-1 text-neutral-500 opacity-0 group-hover:opacity-100 hover:bg-neutral-800 hover:text-neutral-100 focus-visible:opacity-100 data-[state=open]:opacity-100"
							aria-label="Actions for {node.entry.name}"
						>
							<Ellipsis class="size-3.5" />
						</DropdownMenu.Trigger>
						<DropdownMenu.Content
							class="z-50 min-w-40 rounded-md border border-neutral-800 bg-neutral-900 p-1 shadow-xl"
							sideOffset={4}
						>
							{#if node.entry.kind === 'directory'}
								<DropdownMenu.Item class={item} onSelect={() => requestCreate(node.entry.path)}>
									<FilePlus class="size-3.5" /> New file…
								</DropdownMenu.Item>
								<DropdownMenu.Separator class="my-1 h-px bg-neutral-800" />
							{/if}
							<DropdownMenu.Item class={item} onSelect={() => requestRename(node.entry)}>
								<Pencil class="size-3.5" /> Rename…
							</DropdownMenu.Item>
							<DropdownMenu.Item class={item} onSelect={() => requestDelete(node.entry)}>
								<Trash2 class="size-3.5" /> Delete…
							</DropdownMenu.Item>
						</DropdownMenu.Content>
					</DropdownMenu.Root>
				</ContextMenu.Trigger>

				<ContextMenu.Content
					class="z-50 min-w-40 rounded-md border border-neutral-800 bg-neutral-900 p-1 shadow-xl"
				>
					{#if node.entry.kind === 'directory'}
						<ContextMenu.Item class={item} onSelect={() => requestCreate(node.entry.path)}>
							<FilePlus class="size-3.5" /> New file…
						</ContextMenu.Item>
						<ContextMenu.Separator class="my-1 h-px bg-neutral-800" />
					{/if}
					<ContextMenu.Item class={item} onSelect={() => requestRename(node.entry)}>
						<Pencil class="size-3.5" /> Rename…
					</ContextMenu.Item>
					<ContextMenu.Item class={item} onSelect={() => requestDelete(node.entry)}>
						<Trash2 class="size-3.5" /> Delete…
					</ContextMenu.Item>
				</ContextMenu.Content>
			</ContextMenu.Root>
		{/each}
	</div>
</div>

<PromptDialog
	bind:open={createOpen}
	title="New file"
	description="The path is relative to the workspace root. Missing folders are created."
	label="Path"
	placeholder="lib/helpers.fxc"
	initial={createDir ? `${createDir}/` : ''}
	confirmLabel="Create"
	onconfirm={(path) => oncreate(path)}
/>

<PromptDialog
	bind:open={renameOpen}
	title="Rename"
	description="Enter the new workspace-relative path."
	label="Path"
	initial={renameEntry?.path ?? ''}
	confirmLabel="Rename"
	validate={(value) => (value === renameEntry?.path ? 'That is already the name.' : null)}
	onconfirm={(value) => {
		if (renameEntry) onrename(renameEntry.path, value);
	}}
/>

<ConfirmDialog
	bind:open={deleteOpen}
	title="Delete {deleteEntry?.kind ?? 'entry'}?"
	message="Delete “{deleteEntry?.path ?? ''}”? This cannot be undone."
	confirmLabel="Delete"
	danger
	onconfirm={() => {
		if (deleteEntry) ondelete(deleteEntry);
	}}
/>
