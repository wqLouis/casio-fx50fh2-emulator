/**
 * Presentation helpers shared by the workbench panels.
 *
 * The wasm client speaks LSP positions (zero-based) and wraps every failure in
 * the same envelope; turning both into something a person reads is done once
 * here so the Problems panel, the run panel and the status bar agree.
 */

import type { FxError, Range } from '$lib/fx50';

/** A range as `line:column`, one-based because that is what people count. */
export function formatRange(range: Range | undefined): string | null {
	if (!range) return null;
	return `${range.start.line + 1}:${range.start.character + 1}`;
}

/** The error envelope as one sentence, with the file and position when known. */
export function formatError(error: FxError): string {
	const position = formatRange(error.range);
	const where = [error.file, position].filter(Boolean).join(':');
	return where ? `${where}: ${error.message}` : error.message;
}
