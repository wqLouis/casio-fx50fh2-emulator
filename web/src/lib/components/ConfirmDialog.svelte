<script lang="ts">
	/**
	 * A yes/no confirmation, on top of Bits UI's alert dialog.
	 *
	 * Alert dialog rather than dialog because the caller is about to discard
	 * something — unsaved text, a file — and a screen reader should announce it
	 * as such. The action element closes the dialog; `onconfirm` is the work.
	 */
	import { AlertDialog } from 'bits-ui';

	interface Props {
		open?: boolean;
		title: string;
		message: string;
		confirmLabel?: string;
		/** Paint the confirm button as destructive. */
		danger?: boolean;
		onconfirm: () => void;
	}

	let {
		open = $bindable(false),
		title,
		message,
		confirmLabel = 'Confirm',
		danger = false,
		onconfirm
	}: Props = $props();

	const action =
		'rounded-md px-3 py-1.5 text-sm font-medium transition-colors focus-visible:ring-2 focus-visible:outline-none';
</script>

<AlertDialog.Root bind:open>
	<AlertDialog.Portal>
		<AlertDialog.Overlay class="fixed inset-0 z-40 bg-black/60" />
		<AlertDialog.Content
			class="fixed top-1/2 left-1/2 z-50 w-[min(28rem,calc(100vw-2rem))] -translate-x-1/2 -translate-y-1/2 rounded-lg border border-neutral-800 bg-neutral-900 p-5 text-neutral-100 shadow-xl"
		>
			<AlertDialog.Title class="text-sm font-semibold">{title}</AlertDialog.Title>
			<AlertDialog.Description class="mt-2 text-sm text-neutral-400">
				{message}
			</AlertDialog.Description>
			<div class="mt-5 flex justify-end gap-2">
				<AlertDialog.Cancel
					class="{action} text-neutral-300 hover:bg-neutral-800 focus-visible:ring-neutral-600"
				>
					Cancel
				</AlertDialog.Cancel>
				<button
					type="button"
					class="{action} {danger
						? 'bg-red-600 text-white hover:bg-red-500 focus-visible:ring-red-400'
						: 'bg-neutral-100 text-neutral-900 hover:bg-white focus-visible:ring-neutral-300'}"
					onclick={() => {
						onconfirm();
						open = false;
					}}
				>
					{confirmLabel}
				</button>
			</div>
		</AlertDialog.Content>
	</AlertDialog.Portal>
</AlertDialog.Root>
