/**
 * The shapes the panels exchange with the shell.
 *
 * `LucideIcon` is imported as a *type* only: it is erased before bundling, so it
 * does not drag the package's ~1700-icon barrel into the build the way a value
 * import would.
 */

import type { LucideIcon } from '@lucide/svelte';

import type { MachineState } from '$lib/fx50';

/** Which way a split's panes are laid out. Horizontal means side by side. */
export type SplitOrientation = 'horizontal' | 'vertical';

/**
 * One panel the toolbar can show or hide.
 *
 * The shell owns the list — the panels that exist, what they are called, and
 * which icon represents them; the toolbar only renders it and reports toggles.
 * `visible` is read on every render, so the shell can keep it in a store.
 */
export interface PanelDescriptor {
	/** Stable identity, shared with the layout store. */
	id: string;
	/** Human label, used as the button text and the accessible name. */
	label: string;
	/** Icon component, imported per file from `@lucide/svelte/icons/...`. */
	icon: LucideIcon;
	/** Whether the panel is currently shown. */
	visible: boolean;
}

/**
 * One item in the REPL scrollback.
 *
 * Either a line the user ran — `source` plus what it displayed or the error it
 * produced — or a neutral system note (a reset, or a cleared screen). Keeping
 * both in one list preserves the order a person sees.
 */
export interface ReplLine {
	id: number;
	/** The echoed input. Empty for a system note. */
	source: string;
	/** The `◢` displays, in order. */
	outputs: string[];
	/** The failure, rendered as a failure. `null` when the entry succeeded. */
	error: string | null;
	/** A neutral system message. `null` for a normal entry. */
	note: string | null;
	/** The session state after a successful entry, when there is one. */
	state: MachineState | null;
}
