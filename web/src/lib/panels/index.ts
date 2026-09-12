/**
 * `$lib/panels` — the panel primitives the app shell composes.
 *
 * The components take data and report events; the shell owns which panels exist,
 * what is open, and where the splits sit. `layout` is the one piece of state
 * that ships with them, because every shell would otherwise reimplement it; it
 * is inert until `layout.restore()` is called from `onMount`.
 */

export { default as SplitPane } from './SplitPane.svelte';
export { default as Toolbar } from './Toolbar.svelte';
export { default as ReplPanel } from './ReplPanel.svelte';

export { LayoutStore, layout, type LayoutStoreOptions } from './layout.svelte';
export {
	LAYOUT_STORAGE_KEY,
	LAYOUT_VERSION,
	emptyLayout,
	normalizeLayout,
	parseLayout,
	serializeLayout,
	withPanel,
	withSize,
	type LayoutData
} from './layout';
export {
	clampPaneSize,
	clampRatio,
	defaultPaneSize,
	splitBounds,
	type SplitBounds,
	type SplitInput
} from './split';
export {
	describeThrown,
	formatReplError,
	historyBack,
	historyForward,
	parseInputs,
	pushHistory,
	withInputHint
} from './repl';
export type { PanelDescriptor, ReplLine, SplitOrientation } from './types';
