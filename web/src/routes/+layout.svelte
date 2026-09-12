<script lang="ts">
	/**
	 * The site shell: the navigation bar, the theme switch, and the page.
	 *
	 * The workbench fills the viewport below the bar rather than owning the whole
	 * window, so the docs and the workbench share one frame instead of each
	 * inventing its own chrome. The bar is deliberately thin — the workbench is
	 * the thing people come for — and it is the only place the theme can be
	 * changed, so it exists on every route.
	 *
	 * `theme.restore()` belongs here rather than in a page: it registers a
	 * `matchMedia` listener that must outlive any one route.
	 */
	import { onMount } from 'svelte';
	import { base } from '$app/paths';
	import { page } from '$app/state';
	import BookOpen from '@lucide/svelte/icons/book-open';
	import Calculator from '@lucide/svelte/icons/calculator';
	import Moon from '@lucide/svelte/icons/moon';
	import Sun from '@lucide/svelte/icons/sun';

	import './layout.css';
	import favicon from '$lib/assets/favicon.svg';
	import ThemePicker from '$lib/components/ThemePicker.svelte';
	import { theme } from '$lib/theme.svelte';

	let { children } = $props();

	onMount(() => theme.restore());

	const links = [
		{ href: `${base}/`, label: 'Workbench', icon: Calculator },
		{ href: `${base}/docs/`, label: 'Docs', icon: BookOpen }
	];

	/** The current path, without a trailing slash, so `/docs/` matches `/docs`. */
	const here = $derived(page.url.pathname.replace(/\/+$/, '') || '/');
</script>

<svelte:head><link rel="icon" href={favicon} /></svelte:head>

<div class="flex h-screen flex-col overflow-hidden bg-neutral-950 text-neutral-100 antialiased">
	<nav
		class="flex shrink-0 items-center gap-0.5 border-b border-neutral-800 bg-neutral-950 px-2 py-1.5"
	>
		<a
			href={`${base}/`}
			class="mr-2 flex items-center gap-1.5 rounded px-1.5 py-1 text-xs font-semibold tracking-tight text-neutral-200 hover:text-neutral-50"
		>
			<Calculator class="size-4 text-neutral-400" />
			fx-50FH II
		</a>

		{#each links as link (link.href)}
			{@const current = here === link.href.replace(/\/+$/, '')}
			<a
				href={link.href}
				aria-current={current ? 'page' : undefined}
				class="flex items-center gap-1.5 rounded px-2 py-1 text-xs font-medium transition-colors {current
					? 'bg-neutral-800 text-neutral-100'
					: 'text-neutral-400 hover:bg-neutral-900 hover:text-neutral-200'}"
			>
				<link.icon class="size-3.5" />
				{link.label}
			</a>
		{/each}

		<!-- A quick flip between light and dark, and the full list beside it. The
		     shortcut keeps the theme family where it can, so Catppuccin Mocha flips
		     to Latte rather than discarding what was picked. -->
		<div class="ml-auto flex items-center gap-1.5">
			<button
				type="button"
				onclick={() => theme.toggle()}
				aria-label="Switch between light and dark"
				title="Switch between light and dark"
				class="flex items-center rounded border border-neutral-800 p-1.5 text-neutral-400 transition-colors hover:bg-neutral-800 hover:text-neutral-100"
			>
				{#if theme.appearance === 'dark'}
					<Sun class="size-3.5" />
				{:else}
					<Moon class="size-3.5" />
				{/if}
			</button>

			<ThemePicker />
		</div>
	</nav>

	<main class="min-h-0 flex-1">
		{@render children()}
	</main>
</div>
