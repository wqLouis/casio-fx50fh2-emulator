<script lang="ts">
	/**
	 * The top toolbar: one toggle per panel, plus a menu with the same list.
	 *
	 * It is deliberately generic. The shell decides which panels exist, what they
	 * are called and which icon they carry; this renders that list and reports
	 * `ontoggle(id)`. It holds no state of its own.
	 *
	 * The buttons are native, carry `aria-pressed` so their on/off state is
	 * announced, and the bar is a `role="toolbar"` with Left/Right/Home/End
	 * moving focus between them. The menu is Bits UI's — a menu is exactly the
	 * shape it exists for.
	 */
	import { DropdownMenu } from 'bits-ui';
	import Check from '@lucide/svelte/icons/check';
	import ChevronDown from '@lucide/svelte/icons/chevron-down';

	import type { PanelDescriptor } from './types';

	interface Props {
		/** The panels the shell can show or hide, in toolbar order. */
		panels: PanelDescriptor[];
		/** Called with a panel id when its button or menu item is activated. */
		ontoggle: (id: string) => void;
		/** Accessible name for the toolbar and its menu. */
		label?: string;
	}

	let { panels, ontoggle, label = 'Panels' }: Props = $props();

	let barEl: HTMLDivElement | null = $state(null);

	const buttonBase =
		'inline-flex items-center gap-1.5 rounded-md px-2 py-1 text-xs font-medium outline-none transition-colors focus-visible:ring-2 focus-visible:ring-neutral-500';

	function buttonClass(visible: boolean): string {
		return `${buttonBase} ${
			visible
				? 'bg-neutral-800 text-neutral-100'
				: 'text-neutral-400 hover:bg-neutral-800/60 hover:text-neutral-200'
		}`;
	}

	/** Roving focus: Left/Right step through the bar's buttons, Home/End jump. */
	function onKeyDown(event: KeyboardEvent) {
		const keys = ['ArrowLeft', 'ArrowRight', 'Home', 'End'];
		if (!keys.includes(event.key)) return;
		const buttons = Array.from(
			barEl?.querySelectorAll<HTMLButtonElement>('button:not([disabled])') ?? []
		);
		if (buttons.length === 0) return;
		const current = buttons.indexOf(event.target as HTMLButtonElement);
		let next = current;
		if (event.key === 'ArrowLeft') next = current <= 0 ? buttons.length - 1 : current - 1;
		else if (event.key === 'ArrowRight') next = current >= buttons.length - 1 ? 0 : current + 1;
		else if (event.key === 'Home') next = 0;
		else if (event.key === 'End') next = buttons.length - 1;
		event.preventDefault();
		buttons[next]?.focus();
	}
</script>

<div
	bind:this={barEl}
	role="toolbar"
	aria-label={label}
	aria-orientation="horizontal"
	tabindex="-1"
	onkeydown={onKeyDown}
	class="flex flex-wrap items-center gap-1"
>
	{#each panels as panel (panel.id)}
		{@const Icon = panel.icon}
		<button
			type="button"
			aria-pressed={panel.visible}
			aria-label={`${panel.visible ? 'Hide' : 'Show'} ${panel.label}`}
			title={`${panel.visible ? 'Hide' : 'Show'} ${panel.label}`}
			onclick={() => ontoggle(panel.id)}
			class={buttonClass(panel.visible)}
		>
			<Icon class="size-3.5" />
			<span class="hidden sm:inline">{panel.label}</span>
		</button>
	{/each}

	<DropdownMenu.Root>
		<DropdownMenu.Trigger
			class="{buttonBase} text-neutral-400 hover:bg-neutral-800/60 hover:text-neutral-200"
			aria-label="Choose panels"
			title="Choose panels"
		>
			<ChevronDown class="size-3.5" />
			<span class="hidden sm:inline">Panels</span>
		</DropdownMenu.Trigger>
		<DropdownMenu.Portal>
			<DropdownMenu.Content
				sideOffset={6}
				align="start"
				class="z-50 min-w-44 rounded-md border border-neutral-800 bg-neutral-900 p-1 shadow-lg"
			>
				<DropdownMenu.GroupHeading
					class="px-2 py-1 text-[10px] font-semibold tracking-wide text-neutral-500 uppercase"
				>
					Panels
				</DropdownMenu.GroupHeading>
				{#each panels as panel (panel.id)}
					{@const Icon = panel.icon}
					<DropdownMenu.CheckboxItem
						checked={panel.visible}
						onCheckedChange={() => ontoggle(panel.id)}
						class="flex cursor-pointer items-center gap-2 rounded px-2 py-1.5 text-xs text-neutral-200 outline-none select-none data-[highlighted]:bg-neutral-800"
					>
						<Icon class="size-3.5" />
						<span>{panel.label}</span>
						<Check
							class="ml-auto size-3.5 transition-opacity {panel.visible
								? 'opacity-100'
								: 'opacity-0'}"
						/>
					</DropdownMenu.CheckboxItem>
				{/each}
			</DropdownMenu.Content>
		</DropdownMenu.Portal>
	</DropdownMenu.Root>
</div>
