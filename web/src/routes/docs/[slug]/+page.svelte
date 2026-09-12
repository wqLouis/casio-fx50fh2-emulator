<script lang="ts">
	/**
	 * One rendered document.
	 *
	 * The body is already HTML, built from the repository's markdown by
	 * `web/scripts/bundle-docs.ts`, so it is trusted repository content and is
	 * injected directly. The styles travel with the route and are written
	 * against Tailwind's colour variables, so they follow the light theme.
	 */
	import { base } from '$app/paths';
	import ArrowRight from '@lucide/svelte/icons/arrow-right';
	import BookOpen from '@lucide/svelte/icons/book-open';
	import Calculator from '@lucide/svelte/icons/calculator';
	import FileCode from '@lucide/svelte/icons/file-code';
	import FlaskConical from '@lucide/svelte/icons/flask-conical';

	import '../docs.css';

	let { data } = $props();

	const link =
		'inline-flex items-center gap-1.5 rounded-md border border-neutral-800 px-2.5 py-1.5 text-xs font-medium text-neutral-300 transition-colors hover:bg-neutral-800 hover:text-neutral-100';
</script>

<svelte:head>
	<title>{data.doc.title} · fx-50FH II docs</title>
	<meta name="description" content={data.doc.description} />
</svelte:head>

<div class="h-full overflow-auto bg-neutral-950 text-neutral-100 antialiased">
	<div class="mx-auto max-w-3xl px-6 py-10">
		<nav class="flex flex-wrap items-center justify-between gap-3" aria-label="Breadcrumb">
			<ol class="flex items-center gap-1.5 text-xs text-neutral-500">
				<li>
					<a class="flex items-center gap-1.5 hover:text-neutral-200" href={`${base}/docs/`}>
						<BookOpen class="size-3.5" /> Docs
					</a>
				</li>
				<li aria-hidden="true">/</li>
				<li class="text-neutral-300">{data.doc.title}</li>
			</ol>

			<div class="flex items-center gap-2">
				<a class={link} href={`${base}/`}>
					<Calculator class="size-3.5" /> Workbench
				</a>
				<a class={link} href={`${base}/docs/examples/`}>
					<FlaskConical class="size-3.5" /> Examples
				</a>
			</div>
		</nav>

		<article class="doc-prose mt-8">{@html data.doc.html}</article>

		<footer class="mt-12 flex flex-wrap items-center gap-3 border-t border-neutral-800 pt-6">
			<a class={link} href={`${base}/docs/`}>
				<BookOpen class="size-3.5" /> All documentation
			</a>
			<a class={link} href={`${base}/docs/examples/`}>
				<FileCode class="size-3.5" /> Browse the examples
			</a>
			<a
				class="ml-auto flex items-center gap-1.5 text-xs text-neutral-500 hover:text-neutral-200"
				href={`${base}/`}
			>
				Open the workbench <ArrowRight class="size-3.5" />
			</a>
		</footer>
	</div>
</div>
