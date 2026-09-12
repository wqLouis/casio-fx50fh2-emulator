<script lang="ts">
	/**
	 * A single-line text prompt, used for creating and renaming files.
	 *
	 * The dialog owns no meaning: the caller supplies the title, the validation
	 * sentence and what to do with the value. It focuses the field when it opens
	 * so the keyboard path is one keystroke shorter, and submits on Enter.
	 */
	import { Dialog } from 'bits-ui';

	interface Props {
		open?: boolean;
		title: string;
		description?: string;
		label: string;
		placeholder?: string;
		initial?: string;
		confirmLabel?: string;
		/** Return an error sentence to block submission, or `null` to allow it. */
		validate?: (value: string) => string | null;
		onconfirm: (value: string) => void;
	}

	let {
		open = $bindable(false),
		title,
		description,
		label,
		placeholder,
		initial = '',
		confirmLabel = 'Confirm',
		validate,
		onconfirm
	}: Props = $props();

	let value = $state('');
	let field: HTMLInputElement | null = $state(null);
	let problem = $state<string | null>(null);

	// Reset to the caller's initial value on each open, rather than once at
	// construction, because the same dialog instance is reused for every file.
	$effect(() => {
		if (open) {
			value = initial;
			problem = null;
		}
	});

	function submit(event: SubmitEvent) {
		event.preventDefault();
		const trimmed = value.trim();
		const error = trimmed === '' ? 'Enter a name.' : (validate?.(trimmed) ?? null);
		if (error) {
			problem = error;
			return;
		}
		onconfirm(trimmed);
		open = false;
	}
</script>

<Dialog.Root bind:open>
	<Dialog.Portal>
		<Dialog.Overlay class="fixed inset-0 z-40 bg-black/60" />
		<Dialog.Content
			class="fixed top-1/2 left-1/2 z-50 w-[min(28rem,calc(100vw-2rem))] -translate-x-1/2 -translate-y-1/2 rounded-lg border border-neutral-800 bg-neutral-900 p-5 text-neutral-100 shadow-xl"
			onOpenAutoFocus={(event) => {
				event.preventDefault();
				field?.focus();
				field?.select();
			}}
		>
			<Dialog.Title class="text-sm font-semibold">{title}</Dialog.Title>
			{#if description}
				<Dialog.Description class="mt-2 text-sm text-neutral-400">
					{description}
				</Dialog.Description>
			{/if}
			<form class="mt-4" onsubmit={submit}>
				<label class="block text-xs font-medium text-neutral-400" for="prompt-value">{label}</label>
				<input
					id="prompt-value"
					bind:this={field}
					bind:value
					{placeholder}
					autocomplete="off"
					spellcheck="false"
					class="mt-1 w-full rounded-md border border-neutral-700 bg-neutral-950 px-3 py-1.5 font-mono text-sm text-neutral-100 focus:border-neutral-500 focus:outline-none"
				/>
				{#if problem}
					<p class="mt-2 text-xs text-red-400">{problem}</p>
				{/if}
				<div class="mt-5 flex justify-end gap-2">
					<Dialog.Close
						class="rounded-md px-3 py-1.5 text-sm font-medium text-neutral-300 transition-colors hover:bg-neutral-800 focus-visible:ring-2 focus-visible:ring-neutral-600 focus-visible:outline-none"
					>
						Cancel
					</Dialog.Close>
					<button
						type="submit"
						class="rounded-md bg-neutral-100 px-3 py-1.5 text-sm font-medium text-neutral-900 transition-colors hover:bg-white focus-visible:ring-2 focus-visible:ring-neutral-300 focus-visible:outline-none"
					>
						{confirmLabel}
					</button>
				</div>
			</form>
		</Dialog.Content>
	</Dialog.Portal>
</Dialog.Root>
