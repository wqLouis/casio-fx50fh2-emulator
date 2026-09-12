<script lang="ts">
	/**
	 * The examples gallery.
	 *
	 * Every file under `examples/`, both languages, with its source and — for
	 * `.fxc` — the cases its own `#tests` table carries. The page is a reading
	 * copy of the repository, generated at build time, so the workbench stays
	 * the place you edit and this stays the place you look.
	 */
	import { base } from '$app/paths';
	import BookOpen from '@lucide/svelte/icons/book-open';
	import Calculator from '@lucide/svelte/icons/calculator';
	import Check from '@lucide/svelte/icons/check';
	import Copy from '@lucide/svelte/icons/copy';
	import FileCode from '@lucide/svelte/icons/file-code';
	import FlaskConical from '@lucide/svelte/icons/flask-conical';

	import { exampleAnchor } from '$lib/docs.generated';

	let { data } = $props();

	/** The path of the example most recently copied, for the button's label. */
	let copied = $state<string | null>(null);

	const link =
		'inline-flex items-center gap-1.5 rounded-md border border-neutral-800 px-2.5 py-1.5 text-xs font-medium text-neutral-300 transition-colors hover:bg-neutral-800 hover:text-neutral-100';

	const badge = 'rounded-full px-2 py-0.5 text-[10px] font-medium tracking-wide uppercase';

	async function copy(path: string, source: string) {
		try {
			await navigator.clipboard.writeText(source);
			copied = path;
			setTimeout(() => {
				if (copied === path) copied = null;
			}, 1500);
		} catch {
			copied = null;
		}
	}
</script>

<svelte:head>
	<title>Examples · fx-50FH II docs</title>
	<meta
		name="description"
		content="Every .fxc and PRGM example, with source and embedded test cases."
	/>
</svelte:head>

<div class="h-full overflow-auto bg-neutral-950 text-neutral-100 antialiased">
	<div class="mx-auto max-w-4xl px-6 py-10">
		<nav class="flex flex-wrap items-center justify-between gap-3" aria-label="Breadcrumb">
			<ol class="flex items-center gap-1.5 text-xs text-neutral-500">
				<li>
					<a class="flex items-center gap-1.5 hover:text-neutral-200" href={`${base}/docs/`}>
						<BookOpen class="size-3.5" /> Docs
					</a>
				</li>
				<li aria-hidden="true">/</li>
				<li class="text-neutral-300">Examples</li>
			</ol>

			<div class="flex items-center gap-2">
				<a class={link} href={`${base}/`}>
					<Calculator class="size-3.5" /> Workbench
				</a>
				<a class={link} href={`${base}/docs/fxc/`}>
					<FileCode class="size-3.5" /> .fxc language
				</a>
			</div>
		</nav>

		<header class="mt-8 border-b border-neutral-800 pb-6">
			<p
				class="flex items-center gap-1.5 text-xs font-medium tracking-wide text-neutral-500 uppercase"
			>
				<FlaskConical class="size-3.5" /> Examples
			</p>
			<h1 class="mt-2 text-2xl font-semibold tracking-tight">Worked programs</h1>
			<p class="mt-2 max-w-2xl text-sm leading-relaxed text-neutral-400">
				Every program the repository ships. The <code class="font-mono text-neutral-300">.fxc</code>
				files are the C-like language; the <code class="font-mono text-neutral-300">.fx</code> files
				are the calculator's own PRGM keystrokes. Cases marked below are the program's embedded
				tests, run by <code class="font-mono text-neutral-300">fx50 test</code>.
			</p>
			<p class="mt-3 text-xs text-neutral-500">
				{data.programs.length} programs · {data.programs.filter(
					(program) => program.language === 'fxc'
				).length} in
				<code class="font-mono">.fxc</code>
				· {data.programs.filter((program) => program.language === 'prgm').length} in
				<code class="font-mono">.fx</code>
				· {Object.keys(data.libraries).length} libraries
			</p>
		</header>

		<div id={exampleAnchor('examples')} class="mt-8 scroll-mt-4 space-y-6">
			{#each data.programs as program (program.path)}
				<section
					id={exampleAnchor(program.path)}
					class="scroll-mt-4 overflow-hidden rounded-lg border border-neutral-800 bg-neutral-900"
				>
					<header class="flex flex-wrap items-center gap-2 border-b border-neutral-800 px-4 py-3">
						<FileCode class="size-4 shrink-0 text-neutral-500" />
						<h2 class="font-mono text-sm font-semibold text-neutral-100">{program.path}</h2>
						<span
							class="{badge} {program.language === 'fxc'
								? 'bg-sky-500/15 text-sky-400'
								: 'bg-amber-500/15 text-amber-400'}"
						>
							{program.language === 'fxc' ? '.fxc' : '.fx'}
						</span>
						{#if program.tests}
							<span class="{badge} bg-emerald-500/15 text-emerald-400">
								{program.tests.length} test{program.tests.length === 1 ? '' : 's'}
							</span>
						{:else}
							<span class="{badge} bg-neutral-800 text-neutral-400">no tests</span>
						{/if}

						<button
							type="button"
							class="ml-auto inline-flex items-center gap-1.5 rounded-md border border-neutral-700 px-2 py-1 text-[11px] font-medium text-neutral-300 transition-colors hover:bg-neutral-800 hover:text-neutral-100"
							onclick={() => copy(program.path, program.source)}
						>
							{#if copied === program.path}
								<Check class="size-3 text-emerald-400" /> Copied
							{:else}
								<Copy class="size-3" /> Copy
							{/if}
						</button>
					</header>

					{#if program.summary}
						<p class="border-b border-neutral-800 px-4 py-2 text-xs text-neutral-400">
							{program.summary}
						</p>
					{/if}

					{#if program.tests}
						<div class="border-b border-neutral-800 px-4 py-3">
							<h3 class="text-[10px] font-semibold tracking-wide text-neutral-500 uppercase">
								Embedded tests assert
							</h3>
							<ul class="mt-2 space-y-1">
								{#each program.tests as test, index (index)}
									<li class="flex flex-wrap items-baseline gap-x-3 gap-y-0.5 text-xs">
										<span class="text-neutral-300">{test.name}</span>
										<span class="font-mono text-neutral-500">
											{test.error ? `error: ${test.error}` : `→ ${(test.output ?? []).join(', ')}`}
										</span>
										{#if test.input && test.input.length > 0}
											<span class="font-mono text-neutral-600">
												input {test.input.join(', ')}
											</span>
										{/if}
									</li>
								{/each}
							</ul>
						</div>
					{/if}

					<pre
						class="overflow-x-auto p-4 font-mono text-xs leading-relaxed whitespace-pre text-neutral-200"><code
							>{program.source}</code
						></pre>
				</section>
			{/each}
		</div>

		{#if Object.keys(data.libraries).length > 0}
			<section id={exampleAnchor('examples/lib')} class="mt-12 scroll-mt-4">
				<h2 class="text-xs font-semibold tracking-wide text-neutral-500 uppercase">
					Libraries under examples/lib/
				</h2>
				<p class="mt-2 text-xs text-neutral-400">
					Included by name at the top level of a program, then inlined before compiling.
				</p>
				<div class="mt-4 space-y-4">
					{#each Object.entries(data.libraries) as [path, source] (path)}
						<div
							id={exampleAnchor(`examples/${path}`)}
							class="scroll-mt-4 overflow-hidden rounded-lg border border-neutral-800 bg-neutral-900"
						>
							<header class="flex items-center gap-2 border-b border-neutral-800 px-4 py-2">
								<FileCode class="size-4 shrink-0 text-neutral-500" />
								<h3 class="font-mono text-sm font-semibold text-neutral-100">examples/{path}</h3>
								<button
									type="button"
									class="ml-auto inline-flex items-center gap-1.5 rounded-md border border-neutral-700 px-2 py-1 text-[11px] font-medium text-neutral-300 transition-colors hover:bg-neutral-800 hover:text-neutral-100"
									onclick={() => copy(`examples/${path}`, source)}
								>
									{#if copied === `examples/${path}`}
										<Check class="size-3 text-emerald-400" /> Copied
									{:else}
										<Copy class="size-3" /> Copy
									{/if}
								</button>
							</header>
							<pre
								class="overflow-x-auto p-4 font-mono text-xs leading-relaxed whitespace-pre text-neutral-200"><code
									>{source}</code
								></pre>
						</div>
					{/each}
				</div>
			</section>
		{/if}

		<footer class="mt-12 border-t border-neutral-800 pt-6">
			<a class={link} href={`${base}/docs/`}>
				<BookOpen class="size-3.5" /> All documentation
			</a>
		</footer>
	</div>
</div>
