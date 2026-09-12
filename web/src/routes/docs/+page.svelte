<script lang="ts">
	/**
	 * The landing page of the documentation: every published document, grouped.
	 *
	 * It deliberately shows the same links in every group and nothing clever.
	 * The workbench answers "what does this do", this page answers "what is
	 * written down", and the examples page answers "show me".
	 */
	import { base } from '$app/paths';
	import ArrowRight from '@lucide/svelte/icons/arrow-right';
	import BookOpen from '@lucide/svelte/icons/book-open';
	import Calculator from '@lucide/svelte/icons/calculator';
	import FlaskConical from '@lucide/svelte/icons/flask-conical';
	import Sparkles from '@lucide/svelte/icons/sparkles';

	import SkillActions from './SkillActions.svelte';

	let { data } = $props();

	const link =
		'inline-flex items-center gap-1.5 rounded-md border border-neutral-800 px-2.5 py-1.5 text-xs font-medium text-neutral-300 transition-colors hover:bg-neutral-800 hover:text-neutral-100';
</script>

<svelte:head>
	<title>Documentation · fx-50FH II</title>
	<meta
		name="description"
		content="The .fxc language, the calculator's PRGM language, the architecture, and the decisions behind the fx-50FH II toolkit."
	/>
</svelte:head>

<div class="h-full overflow-auto bg-neutral-950 text-neutral-100 antialiased">
	<div class="mx-auto max-w-5xl px-6 py-10">
		<header class="flex flex-wrap items-end justify-between gap-4 border-b border-neutral-800 pb-6">
			<div>
				<p
					class="flex items-center gap-1.5 text-xs font-medium tracking-wide text-neutral-500 uppercase"
				>
					<BookOpen class="size-3.5" /> Documentation
				</p>
				<h1 class="mt-2 text-2xl font-semibold tracking-tight">fx-50FH II</h1>
				<p class="mt-2 max-w-2xl text-sm leading-relaxed text-neutral-400">
					Two languages — the C-like <code class="font-mono text-neutral-300">.fxc</code> front end and
					the calculator's own PRGM keystrokes — plus how the project is built. Every page here is rendered
					from the repository's markdown at build time.
				</p>
			</div>

			<div class="flex items-center gap-2">
				<a class={link} href={`${base}/`}>
					<Calculator class="size-3.5" /> Workbench
				</a>
				<a class={link} href={`${base}/docs/examples/`}>
					<FlaskConical class="size-3.5" /> Examples
				</a>
			</div>
		</header>

		<section class="mt-8 rounded-xl border border-sky-500/30 bg-sky-500/5 p-5">
			<div class="flex flex-wrap items-start justify-between gap-5">
				<div class="max-w-2xl">
					<p
						class="flex items-center gap-1.5 text-xs font-medium tracking-wide text-sky-400 uppercase"
					>
						<Sparkles class="size-3.5" /> Agent skill
					</p>
					<h2 class="mt-2 text-base font-semibold text-neutral-100">
						Hand your agent <code class="font-mono text-sky-300">SKILLS.md</code>
					</h2>
					<p class="mt-1.5 text-sm leading-relaxed text-neutral-400">
						A single self-contained markdown file for writing correct
						<code class="font-mono text-neutral-300">.fxc</code> programs. Copy it to the clipboard or
						download it and hand it to an agent — no other page required.
					</p>
				</div>
				<SkillActions />
			</div>
		</section>

		<div class="mt-8 space-y-10">
			{#each data.groups as group (group.name)}
				<section>
					<h2 class="text-xs font-semibold tracking-wide text-neutral-500 uppercase">
						{group.name}
					</h2>
					<ul class="mt-3 grid gap-3 sm:grid-cols-2">
						{#each group.docs as doc (doc.slug)}
							<li>
								<a
									href={`${base}/docs/${doc.slug}/`}
									class="group flex h-full flex-col rounded-lg border border-neutral-800 bg-neutral-900 p-4 transition-colors hover:border-neutral-700 hover:bg-neutral-800"
								>
									<span class="flex items-center gap-2 text-sm font-semibold text-neutral-100">
										{doc.title}
										<ArrowRight
											class="size-3.5 shrink-0 text-neutral-600 transition-transform group-hover:translate-x-0.5 group-hover:text-neutral-400"
										/>
									</span>
									<span class="mt-1.5 text-xs leading-relaxed text-neutral-400">
										{doc.description}
									</span>
								</a>
							</li>
						{/each}
					</ul>
				</section>
			{/each}
		</div>
	</div>
</div>
