<script lang="ts">
	/**
	 * The interactive run panel: a terminal-like scrollback plus an input line.
	 *
	 * The one thing that must not go wrong is the session. A REPL is defined by
	 * its state carrying across entries — `#mode CMPLX` on one line changes what
	 * the next line means, and `5→A` is still there afterwards — so the session
	 * is opened once, when the module arrives, and closed when it goes away.
	 * Opening one per entry would be simpler and would silently break exactly
	 * that.
	 *
	 * A failing entry is data, not an exception: the module answers `{ok:false}`
	 * and keeps the session as it was, so the panel renders the error in place
	 * and carries on. Clearing the scrollback and resetting the session are two
	 * different actions, and the header says so.
	 */
	import { tick } from 'svelte';
	import Eraser from '@lucide/svelte/icons/eraser';
	import Play from '@lucide/svelte/icons/play';
	import RotateCcw from '@lucide/svelte/icons/rotate-ccw';
	import SquareTerminal from '@lucide/svelte/icons/square-terminal';
	import TriangleAlert from '@lucide/svelte/icons/triangle-alert';

	import type { Fx50, MachineState } from '$lib/fx50';
	import {
		describeThrown,
		formatReplError,
		historyBack,
		historyForward,
		parseInputs,
		pushHistory
	} from './repl';
	import type { ReplLine } from './types';

	/** The interpreter's way of saying the entry reached a `?` with nothing to read. */
	const NEEDS_INPUT = /no input available/;

	interface Props {
		/** The loaded module, or `null` before it is ready. */
		fx?: Fx50 | null;
	}

	let { fx = null }: Props = $props();

	/** How many entries the Up/Down history remembers. */
	const HISTORY_LIMIT = 200;

	let sessionId = $state<number | null>(null);
	let openError = $state<string | null>(null);
	let lines = $state<ReplLine[]>([]);
	let source = $state('');
	let inputsText = $state('');
	/**
	 * The entry stopped at a `?` with no value supplied, waiting to be run once
	 * one arrives.
	 *
	 * A terminal asks for input and waits; refusing outright made `?→A` an error
	 * with a hint pointing at a field above the entry. Now the panel asks, the
	 * next thing typed is the value, and the entry runs.
	 */
	let awaiting = $state<string | null>(null);
	/**
	 * Values already given for the pending entry.
	 *
	 * Each attempt restarts the entry from the beginning, so the values have to
	 * be replayed in order: an entry with two prompts asks twice, and the second
	 * attempt is run with both answers. Nothing from a failed attempt is applied
	 * to the session, so replaying is safe.
	 */
	let answered = $state<number[]>([]);
	let history = $state<string[]>([]);
	let historyIndex = $state<number | null>(null);
	let machine = $state<MachineState | null>(null);
	let busy = $state(false);
	let nextId = 0;

	let scroller = $state<HTMLDivElement | null>(null);
	let sourceEl = $state<HTMLInputElement | null>(null);
	let pinned = $state(true);
	let draft = '';

	const ready = $derived(fx !== null && sessionId !== null);
	const memories = $derived(
		machine
			? Object.entries(machine.memories).map(([name, value]) => ({
					name,
					display: value.display
				}))
			: []
	);

	// Open exactly one session per module instance, and close it on teardown.
	// The cleanup runs before the next effect when `fx` changes, so swapping the
	// module cannot leak a session.
	$effect(() => {
		const module = fx;
		sessionId = null;
		openError = null;
		machine = null;
		if (!module) return;

		let opened: number | null = null;
		try {
			const response = module.replOpen();
			if (response.ok) {
				opened = response.id;
				sessionId = response.id;
			} else {
				openError = formatReplError(response.error);
			}
		} catch (cause) {
			openError = describeThrown(cause);
		}

		return () => {
			if (opened === null) return;
			try {
				module.replClose(opened);
			} catch {
				// The panel is going away; a close failure changes nothing.
			}
		};
	});

	function appendLine(partial: {
		source: string;
		outputs?: string[];
		error?: string | null;
		note?: string | null;
		state?: MachineState | null;
	}) {
		lines = [
			...lines,
			{
				id: nextId++,
				source: partial.source,
				outputs: partial.outputs ?? [],
				error: partial.error ?? null,
				note: partial.note ?? null,
				state: partial.state ?? null
			}
		];
	}

	async function scrollToEnd() {
		await tick();
		const el = scroller;
		if (el) el.scrollTop = el.scrollHeight;
	}

	/** Track whether the user is reading the end, so appends do not yank them back. */
	function onScroll() {
		const el = scroller;
		if (!el) return;
		pinned = el.scrollHeight - el.scrollTop - el.clientHeight <= 32;
	}

	function submit() {
		const module = fx;
		const id = sessionId;
		const text = source.trim();
		if (!module || id === null || text === '' || busy) return;

		// Answering a prompt: what was typed is the value, not a new entry.
		if (awaiting !== null) {
			const values = parseInputs(text);
			if (values === null) {
				appendLine({
					source: text,
					error: 'Inputs must be numbers, separated by spaces or commas.'
				});
				source = '';
				return;
			}
			const entry = awaiting;
			answered = [...answered, ...values];
			source = '';
			inputsText = '';
			evaluate(entry, answered);
			return;
		}

		const inputs = parseInputs(inputsText);
		if (inputs === null) {
			appendLine({
				source: text,
				error: 'Inputs must be numbers, separated by spaces or commas.'
			});
			source = '';
			return;
		}
		source = '';
		inputsText = '';
		evaluate(text, inputs);
	}

	/**
	 * Run one entry and record what it did.
	 *
	 * "No input available for `?`" is not treated as a failure. It means the
	 * entry reached a prompt nobody answered, so the panel asks for the value and
	 * remembers the entry to run again with it. The interpreter only adopts the
	 * environment on success, so a failed attempt leaves nothing half-applied and
	 * running the entry a second time is not a repeat of any effect.
	 */
	function evaluate(text: string, inputs: number[]) {
		const module = fx;
		const id = sessionId;
		if (!module || id === null) return;

		busy = true;
		const wasPinned = pinned;

		try {
			const response = module.replEval({ id, source: text, inputs });
			if (response.ok) {
				awaiting = null;
				answered = [];
				appendLine({
					source: text,
					outputs: response.outputs,
					state: response.state
				});
				machine = response.state;
			} else {
				const message = formatReplError(response.error);
				if (NEEDS_INPUT.test(message)) {
					awaiting = text;
					appendLine({
						source: text,
						note: 'Type the value for `?`, then Enter. Separate several with spaces or commas.'
					});
				} else {
					awaiting = null;
					answered = [];
					appendLine({ source: text, error: message });
				}
			}
		} catch (cause) {
			awaiting = null;
			answered = [];
			appendLine({ source: text, error: describeThrown(cause) });
		}

		history = pushHistory(history, text, HISTORY_LIMIT);
		historyIndex = null;
		draft = '';
		busy = false;

		if (wasPinned) void scrollToEnd();
	}

	/** Clear the scrollback only. The session, its memories and its mode survive. */
	function clearScrollback() {
		lines = [];
		pinned = true;
		appendLine({
			source: '',
			note: 'Scrollback cleared — the session and its memories are untouched.'
		});
		void scrollToEnd();
	}

	/** Start the session over: every memory and display setting is forgotten. */
	function resetSession() {
		const module = fx;
		const id = sessionId;
		if (!module || id === null) return;
		try {
			const response = module.replReset(id);
			if (response.ok) {
				machine = response.state;
				lines = [];
				pinned = true;
				appendLine({
					source: '',
					note: 'Session reset — memories, mode and display settings were cleared.',
					state: response.state
				});
			} else {
				appendLine({ source: '', error: formatReplError(response.error) });
			}
		} catch (cause) {
			appendLine({ source: '', error: describeThrown(cause) });
		}
		void scrollToEnd();
	}

	function navigateHistory(direction: 'up' | 'down') {
		const next =
			direction === 'up'
				? historyBack(history, historyIndex)
				: historyForward(history, historyIndex);

		if (next === null) {
			if (direction === 'down') {
				historyIndex = null;
				source = draft;
			}
			return;
		}

		// Going back from a fresh line remembers the draft, so Down can restore
		// it rather than leaving the user with nothing.
		if (historyIndex === null) draft = source;
		historyIndex = next;
		source = history[next];
	}

	function onSourceKeyDown(event: KeyboardEvent) {
		if (event.key === 'Escape' && awaiting !== null) {
			// Escape abandons the prompt rather than trapping the entry field: a
			// mistyped entry should not have to be answered before it can be fixed.
			event.preventDefault();
			const entry = awaiting;
			awaiting = null;
			answered = [];
			source = '';
			appendLine({ source: entry, note: 'Input cancelled.' });
			return;
		}
		if (event.key === 'ArrowUp') {
			event.preventDefault();
			navigateHistory('up');
		} else if (event.key === 'ArrowDown') {
			event.preventDefault();
			navigateHistory('down');
		}
	}

	/** Ctrl/Cmd+L clears the scrollback from either input; the memories stay. */
	function onFormKeyDown(event: KeyboardEvent) {
		if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'l') {
			event.preventDefault();
			clearScrollback();
		}
	}

	/** Clicking the panel does not steal focus from buttons or the inputs. */
	function onPanelClick(event: MouseEvent) {
		const target = event.target as HTMLElement | null;
		if (target?.closest('button, input, a, [role="menuitem"]')) return;
		sourceEl?.focus();
	}

	const actionButton =
		'inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-[11px] text-neutral-400 transition-colors hover:bg-neutral-800 hover:text-neutral-100 focus-visible:ring-2 focus-visible:ring-neutral-500 focus-visible:outline-none disabled:cursor-not-allowed disabled:text-neutral-600 disabled:hover:bg-transparent';
</script>

<!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
<div class="flex h-full flex-col bg-neutral-950" onclick={onPanelClick}>
	<div class="flex shrink-0 items-center gap-2 border-b border-neutral-800 px-3 py-1.5">
		<SquareTerminal class="size-3.5 text-neutral-500" />
		<h3 class="text-xs font-semibold text-neutral-200">REPL</h3>
		<span class="hidden text-[11px] text-neutral-500 lg:inline">
			Clear keeps the memories; Reset forgets them.
		</span>
		<div class="ml-auto flex items-center gap-1">
			<button
				type="button"
				onclick={clearScrollback}
				class={actionButton}
				title="Clear the scrollback (Ctrl/Cmd+L)"
				aria-label="Clear scrollback"
			>
				<Eraser class="size-3.5" />
				<span class="hidden sm:inline">Clear</span>
			</button>
			<button
				type="button"
				onclick={resetSession}
				disabled={sessionId === null}
				class={actionButton}
				title="Reset the session: forget every memory and display setting"
				aria-label="Reset session"
			>
				<RotateCcw class="size-3.5" />
				<span class="hidden sm:inline">Reset session</span>
			</button>
		</div>
	</div>

	{#if openError}
		<div
			role="alert"
			class="flex items-start gap-2 border-b border-red-900/50 bg-red-950/30 px-3 py-2 text-xs text-red-300"
		>
			<TriangleAlert class="mt-0.5 size-3.5 shrink-0" />
			<span class="font-mono whitespace-pre-wrap">{openError}</span>
		</div>
	{:else if !fx}
		<div class="border-b border-neutral-800 px-3 py-2 text-xs text-neutral-500">
			Waiting for the module to load…
		</div>
	{/if}

	<div
		bind:this={scroller}
		onscroll={onScroll}
		role="log"
		aria-label="REPL output"
		aria-live="polite"
		class="min-h-0 flex-1 overflow-auto px-3 py-2 font-mono text-xs"
	>
		{#if lines.length === 0}
			<p class="max-w-prose leading-relaxed text-neutral-500">
				Type a calculator expression or statement and press Enter. State carries across lines:
				<span class="text-neutral-300">#mode CMPLX</span> changes what the next entry means, and
				<span class="text-neutral-300">5→A</span> stays in memory.
			</p>
		{/if}

		{#each lines as line (line.id)}
			<div class="mb-2">
				{#if line.note}
					<p class="pl-4 text-[11px] text-sky-400/80">{line.note}</p>
				{:else}
					<div class="flex items-start gap-2">
						<span class="text-neutral-600 select-none">›</span>
						<pre class="min-w-0 flex-1 whitespace-pre-wrap text-neutral-200">{line.source ||
								' '}</pre>
					</div>
					{#if line.error}
						<div class="mt-0.5 flex items-start gap-2 pl-4 text-red-400">
							<TriangleAlert class="mt-0.5 size-3.5 shrink-0" />
							<span class="whitespace-pre-wrap">{line.error}</span>
						</div>
					{:else if line.outputs.length > 0}
						{#each line.outputs as output, index (index)}
							<pre class="pl-4 whitespace-pre-wrap text-emerald-300">{output}</pre>
						{/each}
					{:else}
						<p class="pl-4 text-neutral-600">(no output)</p>
					{/if}
				{/if}
			</div>
		{/each}
	</div>

	{#if machine}
		<div
			class="flex flex-wrap items-center gap-x-3 gap-y-1 border-t border-neutral-800 px-3 py-1 text-[11px] text-neutral-500"
		>
			<span>Ans <span class="font-mono text-neutral-200">{machine.ans.display}</span></span>
			{#each memories as memory (memory.name)}
				<span>{memory.name} <span class="font-mono text-neutral-200">{memory.display}</span></span>
			{/each}
			<span class="ml-auto tracking-wide uppercase">{machine.mode}</span>
		</div>
	{/if}

	<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
	<form
		class="flex shrink-0 items-center gap-2 border-t border-neutral-800 px-3 py-2"
		onkeydown={onFormKeyDown}
		onsubmit={(event) => {
			event.preventDefault();
			submit();
		}}
	>
		<!-- The prompt character says which of the two things the field is for:
		     an entry, or the value an entry asked for. -->
		<span
			class="font-mono text-xs select-none {awaiting !== null
				? 'text-amber-400'
				: 'text-neutral-600'}">{awaiting !== null ? '?' : '›'}</span
		>
		<input
			bind:this={sourceEl}
			bind:value={source}
			onkeydown={onSourceKeyDown}
			disabled={!ready}
			aria-label="REPL entry"
			autocomplete="off"
			spellcheck="false"
			placeholder={awaiting !== null
				? 'value for `?` — separate several with spaces or commas'
				: sessionId === null
					? 'No interactive session'
					: 'e.g. 5→A   or   sqrt(2)'}
			class="min-w-0 flex-1 rounded border border-neutral-800 bg-neutral-950 px-2 py-1 font-mono text-xs text-neutral-100 placeholder:text-neutral-600 focus:border-neutral-600 focus:outline-none disabled:text-neutral-600"
		/>
		<input
			bind:value={inputsText}
			disabled={!ready}
			aria-label="Values for the ? prompts, separated by spaces or commas"
			placeholder="? values"
			autocomplete="off"
			spellcheck="false"
			class="w-28 rounded border border-neutral-800 bg-neutral-950 px-2 py-1 font-mono text-xs text-neutral-100 placeholder:text-neutral-600 focus:border-neutral-600 focus:outline-none disabled:text-neutral-600"
		/>
		<button
			type="submit"
			disabled={!ready || source.trim() === ''}
			class="inline-flex items-center gap-1.5 rounded-md bg-emerald-600 px-2.5 py-1 text-xs font-medium text-white transition-colors hover:bg-emerald-500 disabled:cursor-not-allowed disabled:bg-neutral-800 disabled:text-neutral-500"
		>
			<Play class="size-3.5" />
			<span class="hidden sm:inline">Run</span>
		</button>
	</form>
</div>
