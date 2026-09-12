<script lang="ts">
	/**
	 * The code editor for both languages: `.fxc` and the calculator's `.fx` PRGM.
	 *
	 * The whole of CodeMirror lives behind `onMount`. Every route in this app is
	 * prerendered by `@sveltejs/adapter-static`, and `EditorView` reaches for
	 * `document` as soon as it is constructed — so creating it at module scope
	 * would break the build, while creating it in `onMount` means the server
	 * never sees it at all.
	 *
	 * The component is ignorant of the workspace: it is handed text and a path,
	 * and it reports changes. The prop names are the frozen contract in
	 * `types.ts`. The path decides the language; see `languageIdForPath`.
	 */
	import { onMount } from 'svelte';
	import { Compartment, EditorState } from '@codemirror/state';
	import {
		EditorView,
		keymap,
		lineNumbers,
		highlightActiveLine,
		highlightActiveLineGutter,
		highlightSpecialChars,
		drawSelection,
		dropCursor,
		rectangularSelection,
		crosshairCursor,
		hoverTooltip
	} from '@codemirror/view';
	import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
	import {
		autocompletion,
		completionKeymap,
		closeBrackets,
		closeBracketsKeymap
	} from '@codemirror/autocomplete';
	import { linter, lintKeymap, forceLinting } from '@codemirror/lint';
	import { indentOnInput } from '@codemirror/language';
	import { highlightSelectionMatches, searchKeymap } from '@codemirror/search';

	import { languageSupportForPath } from './language';
	import { editorTheme } from './theme';
	import {
		createCompletionSource,
		createHoverSource,
		createLintSource,
		toCodeMirrorDiagnostic,
		type WasmEditorContext
	} from './wasm';
	import type { CodeEditorProps } from './types';

	let {
		value,
		fx = null,
		path = 'main.fxc',
		files,
		diagnostics,
		theme = 'dark',
		onchange,
		onready
	}: CodeEditorProps = $props();

	// `files` and `diagnostics` default to undefined in the contract. Deriving the
	// empty map/list keeps their identity stable, so re-reading them in an effect
	// cannot retrigger that effect forever.
	const filesMap = $derived(files ?? {});
	const diagnosticList = $derived(diagnostics ?? []);

	let host: HTMLDivElement;
	let view: EditorView | null = null;
	// A `$state` flag rather than `$state(view)`: an EditorView must not be
	// proxied, and the effects need something reactive to wait for.
	let ready = $state(false);
	// Set while an external `value` change is applied, so the update listener
	// does not call `onchange` for an edit the user did not make.
	let applyingExternal = false;
	// The language is a compartment, not a fixed extension: the one component
	// instance is reused as the active file changes, so the tokenizer has to be
	// swapped when the path does. `Compartment` is pure state, so constructing it
	// during rendering is SSR-safe.
	const languageCompartment = new Compartment();
	// The palette is an extension as well, so it has to be swapped when the theme
	// changes rather than being fixed at construction.
	const themeCompartment = new Compartment();

	onMount(() => {
		const context = (): WasmEditorContext => ({ fx, path, files: filesMap });
		const lint = createLintSource(context);

		const state = EditorState.create({
			doc: value,
			extensions: [
				lineNumbers(),
				highlightActiveLineGutter(),
				highlightSpecialChars(),
				history(),
				drawSelection(),
				dropCursor(),
				EditorState.allowMultipleSelections.of(true),
				indentOnInput(),
				themeCompartment.of(editorTheme(theme)),
				closeBrackets(),
				rectangularSelection(),
				crosshairCursor(),
				highlightActiveLine(),
				highlightSelectionMatches(),
				autocompletion({ override: [createCompletionSource(context)] }),
				hoverTooltip(createHoverSource(context)),
				// One lint pass per change, from one source of truth. When the caller
				// passes `diagnostics`, the linter renders *those* markers and never
				// calls the wasm transpiler, so a caller that already diagnosed the
				// file does not make the module do the work twice. When it does not
				// pass them, the linter calls the wasm itself. The prop wins.
				linter(
					(editorView) =>
						diagnostics === undefined
							? lint(editorView)
							: diagnosticList.map((item) => toCodeMirrorDiagnostic(item, editorView.state.doc)),
					// The linter debounces internally; the delay is the "do not run a
					// full transpile on every keystroke" part.
					{ delay: 400 }
				),
				languageCompartment.of(languageSupportForPath(path)),
				keymap.of([
					...closeBracketsKeymap,
					...defaultKeymap,
					...searchKeymap,
					...historyKeymap,
					...completionKeymap,
					...lintKeymap,
					indentWithTab
				]),
				EditorView.updateListener.of((update) => {
					if (update.docChanged && !applyingExternal) {
						onchange?.(update.state.doc.toString());
					}
				})
			]
		});

		const created = new EditorView({ state, parent: host });
		view = created;
		ready = true;
		onready?.(created);

		return () => {
			created.destroy();
			view = null;
			ready = false;
		};
	});

	// An externally supplied `value` replaces the document. Comparing against the
	// live document is what keeps this from fighting the user: a change that
	// originated in the editor has already made the two equal, so it is skipped.
	$effect(() => {
		const next = value;
		if (!ready || !view) return;
		if (next === view.state.doc.toString()) return;
		applyingExternal = true;
		view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: next } });
		applyingExternal = false;
	});

	// Swapping the active file has to swap the tokenizer too. The wasm-backed
	// completions, diagnostics and hover read `path` fresh on every request, but
	// the syntax highlighting is an extension and only changes on a reconfigure.
	$effect(() => {
		const next = path;
		if (!ready || !view) return;
		view.dispatch({ effects: languageCompartment.reconfigure(languageSupportForPath(next)) });
	});

	// The palette follows the tokenizer's rule: an extension only changes when it
	// is reconfigured, so the switch has to be explicit.
	$effect(() => {
		const next = theme;
		if (!ready || !view) return;
		view.dispatch({ effects: themeCompartment.reconfigure(editorTheme(next)) });
	});

	// When the module, the include context, or an override's markers change, a
	// lint pass is now possible (or newly meaningful); until a keystroke, nothing
	// would ask. A document edit is a change the linter sees by itself, so it is
	// deliberately not a dependency here.
	$effect(() => {
		void fx;
		void path;
		void filesMap;
		void diagnosticList;
		if (ready && view) forceLinting(view);
	});
</script>

<div
	bind:this={host}
	class="h-full overflow-hidden border border-neutral-800 bg-neutral-900 text-neutral-100"
></div>
