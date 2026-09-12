<script lang="ts">
	/**
	 * A placeholder, not the workbench.
	 *
	 * The editor is the next piece of work. This page exists so the whole
	 * pipeline can be exercised end to end before any of it is built: it proves
	 * Tailwind compiled, the module was fetched from the right place (including
	 * under a `BASE_PATH`), and the TypeScript client decoded a response.
	 */
	import { onMount } from 'svelte';
	// Deep imports rather than the package root: `@lucide/svelte` re-exports
	// about 1700 icons from its barrel, and importing one icon should not make
	// the dev server walk all of them.
	import Calculator from '@lucide/svelte/icons/calculator';
	import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
	import { base } from '$app/paths';

	import { load, type VersionInfo } from '$lib/fx50';

	let info = $state<VersionInfo | null>(null);
	let error = $state<string | null>(null);

	onMount(async () => {
		// The module is a static file, so it lives under the same base path the
		// rest of the site does. `base` is the one thing application code has to
		// know about deployment; nothing else hard-codes a URL.
		try {
			const fx = await load(`${base}/fx_wasm.wasm`);
			const response = fx.version();
			if (response.ok) info = response;
			else error = response.error.message;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : String(cause);
		}
	});
</script>

<svelte:head>
	<title>fx-50FH II workbench</title>
	<meta
		name="description"
		content="Write .fxc for the CASIO fx-50FH II, watch it become PRGM, and run it on the emulator."
	/>
</svelte:head>

<main class="min-h-screen bg-neutral-950 px-6 py-16 text-neutral-100 antialiased">
	<div class="mx-auto max-w-2xl">
		<h1 class="flex items-center gap-3 text-2xl font-semibold">
			<Calculator class="size-6" />
			fx-50FH II workbench
		</h1>

		<p class="mt-2 text-sm text-neutral-400">
			Write <code class="text-neutral-200">.fxc</code>, watch it become PRGM, run it on the
			emulator. The transpiler and interpreter are the Rust crates from this repository, compiled to
			WebAssembly.
		</p>

		<section class="mt-8 rounded-lg border border-neutral-800 bg-neutral-900 p-4 text-sm">
			<h2 class="text-xs font-semibold tracking-wide text-neutral-400 uppercase">Build pipeline</h2>

			{#if info}
				<p class="mt-2">
					Module loaded: <strong>fx50 {info.version}</strong> — {info.limits.programKeys} bytes of program
					storage, {info.limits.memories} memories, {info.limits.constants} constants.
				</p>
			{:else if error}
				<p class="mt-2 flex items-start gap-2 text-red-400">
					<TriangleAlert class="mt-0.5 size-4 shrink-0" />
					<span>Could not load the WebAssembly module: {error}</span>
				</p>
			{:else}
				<p class="mt-2 text-neutral-400">Loading the module…</p>
			{/if}
		</section>

		<p class="mt-6 text-sm text-neutral-400">The editor itself is not built yet.</p>
	</div>
</main>
