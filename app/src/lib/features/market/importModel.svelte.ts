/**
 * Import-tab view model: the paste box, the parse preview
 * (review-before-accept), and the commit. Presentation lives in the tab
 * component; it composes over this state.
 *
 * The commit sends the raw text again rather than echoing the preview:
 * the backend re-parses, so what gets stored always comes from the one
 * parser. The preview here is purely the user's review surface.
 */

import type { MarketCommitResult, MarketPastePreview } from '$lib/api';
import { commitMarketPaste, previewMarketPaste } from '$lib/api';
import { describeError } from '$lib/view/errorState';

export function createImportModel() {
	let text = $state('');
	let preview = $state<MarketPastePreview | null>(null);
	let previewing = $state(false);
	let committing = $state(false);
	let error = $state<string | null>(null);
	let committed = $state<MarketCommitResult | null>(null);

	// Monotonic token so a slow preview for superseded text cannot
	// overwrite the newer preview (or its error) after an edit.
	let previewEpoch = 0;

	const canPreview = $derived(text.trim().length > 0 && !previewing && !committing);
	const canCommit = $derived(
		preview !== null && preview.rows.length > 0 && !previewing && !committing,
	);

	function setText(value: string) {
		text = value;
		// Edited text invalidates the standing preview: the review must
		// always describe exactly what a commit would store.
		preview = null;
		committed = null;
		error = null;
		previewEpoch++;
	}

	async function runPreview() {
		if (!text.trim()) return;
		const epoch = ++previewEpoch;
		previewing = true;
		error = null;
		committed = null;
		try {
			const parsed = await previewMarketPaste(text);
			if (epoch !== previewEpoch) return;
			preview = parsed;
			if (parsed.rows.length === 0) {
				error =
					parsed.skipped.length > 0
						? 'No readable market rows; every line was skipped (see reasons below).'
						: 'Nothing to import; the paste is empty.';
			}
		} catch (e) {
			if (epoch !== previewEpoch) return;
			error = describeError(e, 'Failed to read the paste');
		} finally {
			if (epoch === previewEpoch) previewing = false;
		}
	}

	/** Commits the current text; returns true when observations landed
	 * (the page uses it to refresh the overview). */
	async function commit(): Promise<boolean> {
		if (preview === null || preview.rows.length === 0) return false;
		committing = true;
		error = null;
		try {
			committed = await commitMarketPaste(text);
			text = '';
			preview = null;
			return true;
		} catch (e) {
			error = describeError(e, 'Failed to store the observations');
			return false;
		} finally {
			committing = false;
		}
	}

	return {
		get text() {
			return text;
		},
		setText,
		get preview() {
			return preview;
		},
		get previewing() {
			return previewing;
		},
		get committing() {
			return committing;
		},
		get error() {
			return error;
		},
		get committed() {
			return committed;
		},
		get canPreview() {
			return canPreview;
		},
		get canCommit() {
			return canCommit;
		},
		runPreview,
		commit,
	};
}

export type ImportModel = ReturnType<typeof createImportModel>;
