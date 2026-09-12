<script lang="ts">
	/**
	 * The theme picker: one row per theme, plus "match the system".
	 *
	 * The swatch is the reason this is not a plain `<select>`. It is built by
	 * giving the swatch element the theme's *own* class, so the four squares
	 * inside resolve against that theme's palette through the ordinary Tailwind
	 * utilities — which means no colour is written down twice, and a theme added
	 * to `layout.css` and the registry shows up here correctly with nothing to
	 * keep in sync. A hard-coded preview colour would drift the first time a
	 * palette was adjusted.
	 */
	import { DropdownMenu } from 'bits-ui';
	import Check from '@lucide/svelte/icons/check';
	import Monitor from '@lucide/svelte/icons/monitor';
	import Palette from '@lucide/svelte/icons/palette';

	import { THEMES, theme } from '$lib/theme.svelte';

	const item =
		'flex w-full cursor-pointer items-center gap-2 rounded px-2 py-1.5 text-xs text-neutral-200 outline-none select-none data-[highlighted]:bg-neutral-800';
</script>

<DropdownMenu.Root>
	<DropdownMenu.Trigger
		class="flex items-center gap-1.5 rounded border border-neutral-800 px-2 py-1 text-xs font-medium text-neutral-300 transition-colors hover:bg-neutral-800 hover:text-neutral-100"
		aria-label="Choose a theme"
		title="Theme"
	>
		<Palette class="size-3.5" />
		<span class="hidden sm:inline">{theme.choice === 'system' ? 'System' : theme.info.label}</span>
	</DropdownMenu.Trigger>
	<DropdownMenu.Content
		sideOffset={6}
		align="end"
		class="z-50 max-h-96 min-w-52 overflow-y-auto rounded-md border border-neutral-800 bg-neutral-900 p-1 shadow-lg"
	>
		<DropdownMenu.Item class={item} onSelect={() => theme.set('system')}>
			<Monitor class="size-3.5" />
			<span>Match the system</span>
			{#if theme.choice === 'system'}
				<Check class="ml-auto size-3.5" />
			{/if}
		</DropdownMenu.Item>

		<DropdownMenu.Separator class="my-1 h-px bg-neutral-800" />

		<DropdownMenu.Group>
			<DropdownMenu.GroupHeading
				class="px-2 py-1 text-[10px] font-semibold tracking-wide text-neutral-500 uppercase"
			>
				Themes
			</DropdownMenu.GroupHeading>

			{#each THEMES as entry (entry.id)}
				<DropdownMenu.Item
					class={item}
					onSelect={() => theme.set(entry.id)}
					aria-label={entry.label}
				>
					<span
						class="{entry.id} flex items-center gap-0.5 rounded border border-neutral-700 bg-neutral-950 p-0.5"
					>
						<span class="size-3 rounded-xs bg-neutral-900"></span>
						<span class="size-3 rounded-xs bg-neutral-700"></span>
						<span class="size-3 rounded-xs bg-neutral-200"></span>
						<span class="size-3 rounded-xs bg-sky-500"></span>
					</span>
					<span class="truncate">{entry.label}</span>
					{#if theme.resolved === entry.id && theme.choice !== 'system'}
						<Check class="ml-auto size-3.5" />
					{/if}
				</DropdownMenu.Item>
			{/each}
		</DropdownMenu.Group>
	</DropdownMenu.Content>
</DropdownMenu.Root>
