<script lang="ts">
	/**
	 * The workbench: a workspace on the left, the editor in the middle, and the
	 * panels that show what the Rust module makes of the active file below.
	 *
	 * Everything the UI knows about the language comes back from `$lib/fx50`;
	 * nothing here parses `.fxc`. The one thing this file owns is the editing
	 * session — which files are open, which have unsaved text, and which file is
	 * active — because that is a property of the page and the workspace store
	 * deliberately does not track it.
	 *
	 * All wasm work happens in the browser: the page is prerendered, so the
	 * module is loaded in `onMount` and no browser global is read at module
	 * scope.
	 */
	import { onMount } from 'svelte';
	import { base } from '$app/paths';
	import { Tabs } from 'bits-ui';
	import Calculator from '@lucide/svelte/icons/calculator';
	import Download from '@lucide/svelte/icons/download';
	import FileCode from '@lucide/svelte/icons/file-code';
	import FilePlus from '@lucide/svelte/icons/file-plus';
	import Files from '@lucide/svelte/icons/files';
	import FlaskConical from '@lucide/svelte/icons/flask-conical';
	import FolderInput from '@lucide/svelte/icons/folder-input';
	import FolderOpen from '@lucide/svelte/icons/folder-open';
	import Gauge from '@lucide/svelte/icons/gauge';
	import Hammer from '@lucide/svelte/icons/hammer';
	import MemoryStick from '@lucide/svelte/icons/memory-stick';
	import Play from '@lucide/svelte/icons/play';
	import RotateCw from '@lucide/svelte/icons/rotate-cw';
	import Save from '@lucide/svelte/icons/save';
	import Sigma from '@lucide/svelte/icons/sigma';
	import Terminal from '@lucide/svelte/icons/terminal';
	import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
	import X from '@lucide/svelte/icons/x';

	import {
		load,
		type Diagnostic,
		type Fx50,
		type FxError,
		type MemoryPlan,
		type PhysicalConstant,
		type RunResult,
		type SizeInfo,
		type TestReport,
		type VersionInfo
	} from '$lib/fx50';
	import { workspace } from '$lib/workspace';
	import type { WorkspaceEntry } from '$lib/workspace/types';
	import { CodeEditor } from '$lib/editor';
	import { theme } from '$lib/theme.svelte';
	import { layout, ReplPanel, SplitPane, Toolbar, type PanelDescriptor } from '$lib/panels';

	import ConfirmDialog from '$lib/components/ConfirmDialog.svelte';
	import ConstantsPanel from '$lib/components/ConstantsPanel.svelte';
	import FileTree from '$lib/components/FileTree.svelte';
	import MemoriesPanel from '$lib/components/MemoriesPanel.svelte';
	import OpenTabs from '$lib/components/OpenTabs.svelte';
	import PrgmPanel from '$lib/components/PrgmPanel.svelte';
	import ProblemsPanel from '$lib/components/ProblemsPanel.svelte';
	import RunPanel from '$lib/components/RunPanel.svelte';
	import SizePanel from '$lib/components/SizePanel.svelte';
	import StatusBar from '$lib/components/StatusBar.svelte';
	import TestsPanel from '$lib/components/TestsPanel.svelte';
	import { formatError } from '$lib/components/format';

	/** The file a fresh scratch workspace starts with, so Run has something to do. */
	const STARTER = `// A scratch program on the fx-50FH II.
// Edit it, then press Run in the panel below.

fn main() {
    let a = 3;
    let b = 4;
    print(sqrt(a * a + b * b));
}

#tests = [
  { "name": "3-4-5 triangle", "output": ["5"] }
];
`;

	/**
	 * Every region the shell can show or hide: the file tree, then the panels.
	 *
	 * `files` is in the list even though it is not a tab — the toolbar and the
	 * layout store treat every showable region the same way, so one list drives
	 * both the toolbar and the tab strip, and the two can never disagree about
	 * what exists.
	 */
	const PANEL_LIST = [
		{ id: 'files', label: 'Files', icon: Files, tab: false },
		{ id: 'prgm', label: 'PRGM', icon: FileCode, tab: true },
		{ id: 'size', label: 'Size', icon: Gauge, tab: true },
		{ id: 'memories', label: 'Memories', icon: MemoryStick, tab: true },
		{ id: 'run', label: 'Run', icon: Play, tab: true },
		{ id: 'repl', label: 'REPL', icon: Terminal, tab: true },
		{ id: 'tests', label: 'Tests', icon: FlaskConical, tab: true },
		{ id: 'problems', label: 'Problems', icon: TriangleAlert, tab: true },
		{ id: 'constants', label: 'Constants', icon: Sigma, tab: true }
	];
	const TAB_PANELS = PANEL_LIST.filter((item) => item.tab);

	// ---- module ----------------------------------------------------------
	let fx = $state<Fx50 | null>(null);
	let moduleError = $state<string | null>(null);
	let version = $state<VersionInfo | null>(null);
	let constants = $state<PhysicalConstant[]>([]);

	// ---- editing session -------------------------------------------------
	let openPaths = $state<string[]>([]);
	let activePath = $state<string | null>(null);
	let buffers = $state<Record<string, string>>({});
	let savedText = $state<Record<string, string>>({});
	let diskFiles = $state<Record<string, string>>({});

	// ---- compile results -------------------------------------------------
	let prgm = $state<string | null>(null);
	let size = $state<SizeInfo | null>(null);
	let regs = $state<MemoryPlan | null>(null);
	let diagnostics = $state<Diagnostic[]>([]);
	let compileError = $state<FxError | null>(null);
	let ascii = $state(false);
	/**
	 * Whether the build runs the transpiler's size optimisations.
	 *
	 * On by default, matching the transpiler itself. It is exposed because this
	 * machine has 680 bytes of program storage shared by all four program areas,
	 * so the difference between the optimised and unoptimised translation is
	 * worth being able to see rather than take on faith.
	 */
	let optimize = $state(true);
	/**
	 * What the panels' current listing was built from.
	 *
	 * Because the build is manual, the panels can be showing a listing that no
	 * longer matches the editor. Comparing this against the live signature is how
	 * that is noticed, and it is the only reason the build records it.
	 */
	let builtSignature = $state<string | null>(null);

	// ---- run and tests ---------------------------------------------------
	let runResult = $state<RunResult | null>(null);
	let runError = $state<string | null>(null);
	let testReport = $state<TestReport | null>(null);
	let testError = $state<string | null>(null);

	// ---- chrome ----------------------------------------------------------
	let panel = $state<string>('prgm');
	let actionError = $state<string | null>(null);
	let confirmOpen = $state(false);
	let confirmTitle = $state('');
	let confirmMessage = $state('');
	let confirmLabel = $state('Discard');
	let confirmDanger = $state(true);
	let confirmAction: (() => void) | null = null;

	const toolButton =
		'inline-flex items-center gap-1.5 rounded-md border border-neutral-800 px-2.5 py-1.5 text-xs font-medium text-neutral-300 transition-colors hover:bg-neutral-800 hover:text-neutral-100 disabled:cursor-not-allowed disabled:text-neutral-600 disabled:hover:bg-transparent';
	const tabButton =
		'rounded-t-md px-3 py-1.5 text-xs font-medium text-neutral-400 outline-none select-none hover:text-neutral-200 focus-visible:ring-2 focus-visible:ring-neutral-600 data-[state=active]:bg-neutral-900 data-[state=active]:text-neutral-100';

	// ---- derived ---------------------------------------------------------
	/**
	 * The active file's text, which is also what the editor is given.
	 *
	 * It is the buffer itself, not a copy taken when the file was opened. A copy
	 * goes stale the moment the user types, and the editor is rebuilt whenever a
	 * panel toggle changes the shape of the layout — so a recreation would
	 * faithfully restore text from before the last several minutes of work. This
	 * happened: hiding PRGM when it was the last visible panel threw the editor
	 * away and brought it back with the starter program.
	 *
	 * Handing the editor its own text back is safe because `CodeEditor` ignores a
	 * `value` that already matches its document, so no keystroke can reset the
	 * cursor.
	 */
	const activeText = $derived(activePath ? (buffers[activePath] ?? '') : '');
	const dirtySet = $derived(
		new Set(openPaths.filter((path) => (buffers[path] ?? '') !== (savedText[path] ?? '')))
	);
	/** Every workspace file, with unsaved buffers layered over the disk text, for `#include`. */
	const files = $derived.by(() => {
		const map: Record<string, string> = { ...diskFiles };
		for (const path of Object.keys(buffers)) map[path] = buffers[path];
		return map;
	});
	const promptCount = $derived(prgm ? (prgm.match(/\?/g) ?? []).length : 0);
	const compileErrorText = $derived(compileError ? formatError(compileError) : null);
	const banner = $derived(moduleError ?? actionError ?? workspace.error);
	const problemCount = $derived(diagnostics.length + (compileErrorText ? 1 : 0));
	/** What the toolbar renders: every region, and whether it is currently shown. */
	const panelDescriptors = $derived<PanelDescriptor[]>(
		PANEL_LIST.map((item) => ({
			id: item.id,
			label: item.label,
			icon: item.icon,
			visible: layout.isPanelVisible(item.id)
		}))
	);
	/** The tabs to draw. A hidden panel has no tab, so it cannot be selected by accident. */
	const visibleTabs = $derived(TAB_PANELS.filter((item) => layout.isPanelVisible(item.id)));
	const filesVisible = $derived(layout.isPanelVisible('files'));
	/** Everything a build depends on, so that a stale listing is detectable. */
	const buildSignature = $derived(
		activePath === null ? null : [activePath, activeText, ascii, optimize].join('\u0000')
	);
	/** Whether the panels are showing a build of something other than what is in the editor. */
	const buildStale = $derived(builtSignature !== null && builtSignature !== buildSignature);

	// ---- lifecycle -------------------------------------------------------
	onMount(() => {
		// The arrangement is inert until this runs: the store reads `localStorage`
		// at call time, and nothing at module scope may touch a browser global.
		layout.restore();
		let disposed = false;

		void (async () => {
			try {
				const module = await load(`${base}/fx_wasm.wasm`);
				if (disposed) return;
				fx = module;
				const info = module.version();
				if (info.ok) version = info;
				else moduleError = formatError(info.error);
				const list = module.constants();
				if (list.ok) constants = list.constants;
			} catch (cause) {
				if (!disposed) moduleError = cause instanceof Error ? cause.message : String(cause);
			}

			try {
				const restored = await workspace.restore();
				if (disposed) return;
				// Nothing remembered: a scratch workspace is still a workspace, so
				// the page never opens onto a dead end.
				if (!restored && workspace.kind === null) {
					workspace.openMemory({ 'main.fxc': STARTER }, 'Scratch');
				}
				await adoptWorkspace();
			} catch (cause) {
				if (!disposed) actionError = cause instanceof Error ? cause.message : String(cause);
			}
		})();

		return () => {
			disposed = true;
		};
	});

	// Hiding the selected panel would leave its content on screen with no tab to
	// click to leave it, so the selection follows the visible set.
	$effect(() => {
		const tabs = visibleTabs;
		if (tabs.length > 0 && !tabs.some((item) => item.id === panel)) {
			panel = tabs[0].id;
		}
	});

	// Diagnostics stay **live**: a linter whose markers only appear when you press
	// a button is worse than useless, and the Problems panel has to be trustworthy
	// while typing. The **build is not live** — the listing, size and memory plan
	// change only when asked, so they never flicker mid-keystroke and never
	// quietly show something other than what the editor holds. `buildStale` is
	// what keeps that honest.
	$effect(() => {
		const module = fx;
		const entry = activePath;
		const text = activeText;
		const map = files;

		if (!module || !entry) {
			diagnostics = [];
			compileError = null;
			return;
		}

		const timer = setTimeout(() => {
			const diagnosed = module.diagnostics({
				entry,
				files: { ...map, [entry]: text }
			});
			if (diagnosed.ok) {
				diagnostics = diagnosed.diagnostics;
				compileError = null;
			} else {
				diagnostics = [];
				compileError = diagnosed.error;
			}
		}, 250);
		return () => clearTimeout(timer);
	});

	/**
	 * Build the active file into PRGM.
	 *
	 * Explicit by design — see the note above the diagnostics effect. Run and
	 * Tests build first when the listing is stale, so the output can never
	 * describe a different program from the one on screen.
	 */
	function build() {
		const module = fx;
		const entry = activePath;
		if (!module || !entry) return;

		const signature = buildSignature;
		const built = module.transpile({
			entry,
			files: { ...files, [entry]: activeText },
			ascii,
			optimize
		});
		if (built.ok) {
			prgm = built.prgm;
			size = built.size;
			regs = built.regs ?? null;
			compileError = null;
		} else {
			prgm = null;
			size = null;
			regs = null;
			compileError = built.error;
		}
		builtSignature = signature;
	}

	// ---- workspace -------------------------------------------------------
	async function adoptWorkspace() {
		openPaths = [];
		activePath = null;
		buffers = {};
		savedText = {};
		diskFiles = {};
		runResult = null;
		runError = null;
		testReport = null;
		testError = null;
		compileError = null;
		actionError = null;

		await refreshDisk();
		const first = firstFile(diskFiles);
		if (first) await openFile(first);
	}

	async function refreshDisk() {
		try {
			diskFiles = await workspace.snapshot();
		} catch {
			// `workspace.error` carries the reason; the header offers a reconnect.
		}
	}

	function firstFile(map: Record<string, string>): string | null {
		const keys = Object.keys(map);
		if (keys.includes('main.fxc')) return 'main.fxc';
		return keys.find((key) => key.endsWith('.fxc')) ?? keys[0] ?? null;
	}

	async function openFile(path: string) {
		actionError = null;
		if (!(path in buffers)) {
			try {
				const text = await workspace.read(path);
				buffers[path] = text;
				savedText[path] = text;
			} catch (cause) {
				setActionError(cause);
				return;
			}
		}
		if (!openPaths.includes(path)) openPaths = [...openPaths, path];
		activePath = path;
	}

	function selectFile(path: string) {
		if (path === activePath) return;
		if (!(path in buffers)) {
			void openFile(path);
			return;
		}
		activePath = path;
	}

	function onEditorChange(text: string) {
		if (activePath) buffers[activePath] = text;
	}

	function isDirty(path: string): boolean {
		return (buffers[path] ?? '') !== (savedText[path] ?? '');
	}

	function requestClose(path: string) {
		if (isDirty(path)) {
			askConfirm({
				title: 'Discard unsaved changes?',
				message: `“${path}” has changes that have not been saved. Closing it will discard them.`,
				confirmLabel: 'Discard',
				action: () => closeTab(path)
			});
		} else {
			closeTab(path);
		}
	}

	function closeTab(path: string) {
		openPaths = openPaths.filter((open) => open !== path);
		delete buffers[path];
		delete savedText[path];
		if (activePath === path) {
			activePath = openPaths[openPaths.length - 1] ?? null;
		}
	}

	async function saveActive() {
		const path = activePath;
		if (!path || !isDirty(path)) return;
		actionError = null;
		try {
			await workspace.write(path, buffers[path]);
			savedText[path] = buffers[path];
			await refreshDisk();
		} catch (cause) {
			setActionError(cause);
		}
	}

	async function createFile(path: string) {
		actionError = null;
		try {
			await workspace.create(path, '');
			await refreshDisk();
			await openFile(path);
		} catch (cause) {
			setActionError(cause);
		}
	}

	async function renameEntry(from: string, to: string) {
		actionError = null;
		try {
			await workspace.rename(from, to);
			const remap = (map: Record<string, string>) => {
				const next: Record<string, string> = {};
				for (const [key, value] of Object.entries(map)) {
					if (key === from) next[to] = value;
					else if (key.startsWith(`${from}/`)) next[`${to}${key.slice(from.length)}`] = value;
					else next[key] = value;
				}
				return next;
			};
			buffers = remap(buffers);
			savedText = remap(savedText);
			const move = (path: string) =>
				path === from ? to : path.startsWith(`${from}/`) ? `${to}${path.slice(from.length)}` : path;
			openPaths = openPaths.map(move);
			if (activePath) activePath = move(activePath);
			await refreshDisk();
		} catch (cause) {
			setActionError(cause);
		}
	}

	async function deleteEntry(entry: WorkspaceEntry) {
		actionError = null;
		try {
			await workspace.remove(entry.path);
			const prefix = entry.kind === 'directory' ? `${entry.path}/` : entry.path;
			const removed = openPaths.filter((path) => path === entry.path || path.startsWith(prefix));
			openPaths = openPaths.filter((path) => !removed.includes(path));
			for (const path of removed) {
				delete buffers[path];
				delete savedText[path];
			}
			if (activePath && removed.includes(activePath)) {
				activePath = openPaths[openPaths.length - 1] ?? null;
			}
			await refreshDisk();
		} catch (cause) {
			setActionError(cause);
		}
	}

	function guardDiscard(action: () => void) {
		const dirty = openPaths.filter(isDirty);
		if (dirty.length === 0) {
			action();
			return;
		}
		askConfirm({
			title: 'Discard unsaved changes?',
			message: `${dirty.length} open file${dirty.length === 1 ? '' : 's'} will be discarded. This cannot be undone.`,
			confirmLabel: 'Discard and continue',
			action
		});
	}

	function chooseFolder() {
		guardDiscard(() => {
			void (async () => {
				actionError = null;
				const opened = await workspace.openFolder();
				if (opened) await adoptWorkspace();
			})();
		});
	}

	function newScratch() {
		guardDiscard(() => {
			workspace.openMemory({ 'main.fxc': STARTER }, 'Scratch');
			void adoptWorkspace();
		});
	}

	function importFolder() {
		guardDiscard(() => {
			void (async () => {
				actionError = null;
				const imported = await workspace.importFolder();
				if (imported) await adoptWorkspace();
			})();
		});
	}

	async function exportWorkspace() {
		actionError = null;
		try {
			await workspace.exportWorkspace();
		} catch (cause) {
			setActionError(cause);
		}
	}

	async function reconnect() {
		actionError = null;
		const granted = await workspace.reconnect();
		if (granted) await adoptWorkspace();
		else actionError = 'Permission to the folder was not granted.';
	}

	function setActionError(cause: unknown) {
		actionError = cause instanceof Error ? cause.message : String(cause);
	}

	// ---- run and tests ---------------------------------------------------
	function runProgram(inputs: number[]) {
		if (!fx || !activePath) return;
		// Run what is on screen. If the listing is out of date, refresh it first,
		// so the PRGM panel and the displays cannot describe different programs.
		if (buildStale) build();
		// That build may have changed how many `?` the program reads, and the values
		// handed in were collected for the *previous* listing — the panel said "reads
		// no input" when they were filled in. Running anyway feeds the interpreter
		// too few of them, which surfaces as "no input available for `?`" beside a
		// panel that is, by then, asking for one. Stop after the build instead: the
		// fields re-render with the right count and the user fills them in.
		if (promptCount > inputs.length) {
			runResult = null;
			runError = `This build reads ${promptCount} input(s). Fill them in, then press Run.`;
			return;
		}
		const response = fx.run({
			entry: activePath,
			files: { ...files, [activePath]: activeText },
			inputs,
			ascii,
			optimize
		});
		if (response.ok) {
			runResult = response;
			runError = null;
		} else {
			runResult = null;
			runError = formatError(response.error);
		}
	}

	function runTests() {
		if (!fx || !activePath) return;
		const response = fx.tests({
			entry: activePath,
			files: { ...files, [activePath]: activeText }
		});
		if (response.ok) {
			testReport = response;
			testError = null;
		} else {
			testReport = null;
			testError = formatError(response.error);
		}
	}

	// ---- confirmation ----------------------------------------------------
	function askConfirm(options: {
		title: string;
		message: string;
		confirmLabel: string;
		action: () => void;
	}) {
		confirmTitle = options.title;
		confirmMessage = options.message;
		confirmLabel = options.confirmLabel;
		confirmDanger = true;
		confirmAction = options.action;
		confirmOpen = true;
	}

	function runConfirm() {
		const action = confirmAction;
		confirmAction = null;
		action?.();
	}

	function dismissBanner() {
		moduleError = null;
		actionError = null;
		workspace.error = null;
	}
</script>

<svelte:head>
	<title>fx-50FH II workbench</title>
	<meta
		name="description"
		content="Write .fxc for the CASIO fx-50FH II, watch it become PRGM, and run it on the emulator."
	/>
</svelte:head>

<svelte:window
	onkeydown={(event) => {
		if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 's') {
			event.preventDefault();
			void saveActive();
		}
		if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'b') {
			event.preventDefault();
			build();
		}
	}}
	onbeforeunload={(event) => {
		// The browser's own "leave site?" prompt is the only way to catch a
		// reload or a closed tab; the in-page dialogs cover everything else.
		if (dirtySet.size > 0) event.preventDefault();
		// A drag in the last 150ms before an unload would otherwise be lost, since
		// the store coalesces writes.
		layout.persist();
	}}
/>

<div class="flex h-full flex-col overflow-hidden bg-neutral-950 text-neutral-100 antialiased">
	<header class="flex shrink-0 items-center gap-3 border-b border-neutral-800 px-3 py-2">
		<div class="flex items-center gap-2">
			<Calculator class="size-5 text-neutral-400" />
			<h1 class="text-sm font-semibold">fx-50FH II workbench</h1>
		</div>

		<span class="hidden text-xs text-neutral-500 md:inline">
			{workspace.name || 'no workspace'}{workspace.kind ? ` · ${workspace.kind}` : ''}
			{#if workspace.status === 'loading'}(working…){/if}
		</span>

		<!-- Build controls. The build is manual, so this is where the user asks for
		     it, and where the fact that the panels are out of date is stated. -->
		<div class="flex items-center gap-2 border-l border-neutral-800 pl-3">
			<button
				type="button"
				class={toolButton}
				onclick={build}
				disabled={!activePath}
				title="Build the active file into PRGM (Ctrl/Cmd+B)"
			>
				<Hammer class="size-3.5" /> Build
			</button>

			{#if buildStale}
				<span
					class="text-xs text-amber-400"
					title="The editor has changed since these panels were built."
				>
					out of date
				</span>
			{/if}

			<label class="flex cursor-pointer items-center gap-1.5 text-xs text-neutral-400">
				<input type="checkbox" bind:checked={optimize} class="accent-sky-500" />
				Optimize
			</label>
		</div>

		<div class="ml-auto flex items-center gap-2">
			{#if workspace.capabilities.needsPermission}
				<button type="button" class={toolButton} onclick={reconnect}>
					<RotateCw class="size-3.5" /> Reconnect
				</button>
			{/if}

			{#if workspace.capabilities.canOpenFolder}
				<button type="button" class={toolButton} onclick={chooseFolder}>
					<FolderOpen class="size-3.5" /> Open folder
				</button>
			{/if}

			{#if workspace.capabilities.canImportFolder}
				<button type="button" class={toolButton} onclick={importFolder}>
					<FolderInput class="size-3.5" /> Import folder
				</button>
			{/if}

			{#if !workspace.capabilities.canOpenFolder && !workspace.capabilities.canImportFolder}
				<span class="hidden text-xs text-neutral-500 lg:inline">
					This browser cannot reach a folder; the scratch workspace lives in this page.
				</span>
			{/if}

			<button type="button" class={toolButton} onclick={newScratch}>
				<FilePlus class="size-3.5" /> New scratch
			</button>

			{#if workspace.kind === 'browser'}
				<button type="button" class={toolButton} onclick={exportWorkspace}>
					<Download class="size-3.5" /> Export .zip
				</button>
			{/if}

			<button
				type="button"
				class={toolButton}
				onclick={saveActive}
				disabled={!activePath || dirtySet.size === 0}
			>
				<Save class="size-3.5" /> Save
			</button>
		</div>
	</header>

	<!--
		The panel switches are their own strip at the top, deliberately *outside*
		the panel area. Inside it they disappeared the instant the last panel was
		switched off, taking with them the only way to switch one back on — a dead
		end recoverable only by clearing localStorage.
	-->
	<div
		class="flex shrink-0 items-center gap-2 border-b border-neutral-800 bg-neutral-950 px-3 py-1"
	>
		<Toolbar panels={panelDescriptors} ontoggle={(id) => layout.togglePanel(id)} />
		{#if visibleTabs.length === 0}
			<span class="text-xs text-neutral-500"> No panels shown — switch one on to see it. </span>
		{/if}
	</div>

	{#if banner}
		<div
			class="flex shrink-0 items-start gap-2 border-b border-red-900/50 bg-red-950/40 px-3 py-2 text-xs text-red-300"
		>
			<TriangleAlert class="mt-0.5 size-4 shrink-0" />
			<span class="flex-1 font-mono whitespace-pre-wrap">{banner}</span>
			<button
				type="button"
				onclick={dismissBanner}
				aria-label="Dismiss"
				class="rounded p-0.5 hover:bg-red-900/40"
			>
				<X class="size-3.5" />
			</button>
		</div>
	{/if}

	<!--
		The editor and the panels are snippets rather than inline markup so each
		can be rendered either inside a split or on its own, when the thing it
		would be split against is hidden.
	-->
	{#snippet editorPane()}
		<div class="h-full min-h-0">
			{#if activePath}
				<CodeEditor
					value={activeText}
					{fx}
					path={activePath}
					{files}
					{diagnostics}
					theme={theme.resolved}
					onchange={onEditorChange}
				/>
			{:else}
				<div class="flex h-full flex-col items-center justify-center gap-2 text-neutral-500">
					<Calculator class="size-8" />
					<p class="text-sm">Open a file, or start a new scratch program.</p>
				</div>
			{/if}
		</div>
	{/snippet}

	{#snippet panelPane()}
		<section class="flex h-full min-h-0 flex-col bg-neutral-950">
			<Tabs.Root bind:value={panel} class="flex min-h-0 flex-1 flex-col">
				<Tabs.List
					class="flex shrink-0 items-center gap-0.5 overflow-x-auto border-b border-neutral-800 px-2"
				>
					{#each visibleTabs as item (item.id)}
						<Tabs.Trigger value={item.id} class={tabButton}>
							{item.label}
							{#if item.id === 'problems' && problemCount > 0}
								<span class="ml-1.5 rounded-full bg-amber-500/20 px-1.5 text-[10px] text-amber-400"
									>{problemCount}</span
								>
							{/if}
						</Tabs.Trigger>
					{/each}
				</Tabs.List>

				<div class="min-h-0 flex-1 overflow-hidden">
					<Tabs.Content value="prgm" class="h-full">
						<PrgmPanel {prgm} error={compileErrorText} bind:ascii />
					</Tabs.Content>
					<Tabs.Content value="size" class="h-full">
						<SizePanel {size} {optimize} />
					</Tabs.Content>
					<Tabs.Content value="memories" class="h-full">
						<MemoriesPanel {regs} />
					</Tabs.Content>
					<Tabs.Content value="run" class="h-full">
						<RunPanel
							prompts={promptCount}
							result={runResult}
							error={runError}
							disabled={!activePath}
							onrun={runProgram}
						/>
					</Tabs.Content>
					<Tabs.Content value="repl" class="h-full">
						<ReplPanel {fx} />
					</Tabs.Content>
					<Tabs.Content value="tests" class="h-full">
						<TestsPanel
							report={testReport}
							error={testError}
							disabled={!activePath}
							ontest={runTests}
						/>
					</Tabs.Content>
					<Tabs.Content value="problems" class="h-full">
						<ProblemsPanel {diagnostics} compileError={compileErrorText} />
					</Tabs.Content>
					<Tabs.Content value="constants" class="h-full">
						<ConstantsPanel {constants} />
					</Tabs.Content>
				</div>
			</Tabs.Root>
		</section>
	{/snippet}

	{#snippet workArea()}
		<main class="flex h-full min-w-0 flex-1 flex-col">
			<OpenTabs
				tabs={openPaths}
				active={activePath}
				dirty={dirtySet}
				onselect={selectFile}
				onclose={requestClose}
			/>

			<div class="min-h-0 flex-1">
				{#if visibleTabs.length > 0}
					<SplitPane
						orientation="vertical"
						min={140}
						minOther={120}
						defaultRatio={0.6}
						size={layout.sizeFor('editor')}
						onsize={(value) => layout.setSize('editor', value)}
						label="Resize the editor and the panels"
					>
						{#snippet first()}{@render editorPane()}{/snippet}
						{#snippet second()}{@render panelPane()}{/snippet}
					</SplitPane>
				{:else}
					{@render editorPane()}
				{/if}
			</div>
		</main>
	{/snippet}

	<div class="flex min-h-0 flex-1">
		{#if filesVisible}
			<SplitPane
				orientation="horizontal"
				min={160}
				minOther={360}
				defaultRatio={0.2}
				size={layout.sizeFor('sidebar')}
				onsize={(value) => layout.setSize('sidebar', value)}
				label="Resize the file tree"
			>
				{#snippet first()}
					<aside class="h-full border-r border-neutral-800">
						<FileTree
							entries={workspace.entries}
							{activePath}
							dirty={dirtySet}
							onopen={selectFile}
							oncreate={createFile}
							onrename={renameEntry}
							ondelete={deleteEntry}
						/>
					</aside>
				{/snippet}
				{#snippet second()}{@render workArea()}{/snippet}
			</SplitPane>
		{:else}
			{@render workArea()}
		{/if}
	</div>

	<StatusBar
		version={version?.version ?? null}
		workspaceName={workspace.name}
		kind={workspace.kind}
		limits={version?.limits ?? null}
		{problemCount}
		error={banner}
	/>
</div>

<ConfirmDialog
	bind:open={confirmOpen}
	title={confirmTitle}
	message={confirmMessage}
	{confirmLabel}
	danger={confirmDanger}
	onconfirm={runConfirm}
/>
