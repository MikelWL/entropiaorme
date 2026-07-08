/**
 * Self-sizing for the overlay windows: keep a transparent, chromeless
 * window's OS size in step with the content it renders.
 *
 * An overlay webview is sized to whatever its root element measures plus a
 * fixed slack (room for the glass panel's shadow and sub-pixel rounding, so
 * nothing clips). `createWindowSizeSync` owns the loop: coalesce sync
 * requests onto animation frames, measure the root, dedupe against the last
 * applied size, and apply through the current window. A failed apply forgets
 * the last size so the next request retries rather than deduping against a
 * size that never landed.
 */

import { LogicalSize } from '@tauri-apps/api/dpi';
import { getCurrentWindow } from '@tauri-apps/api/window';

const WINDOW_SIZE_SLACK = 36;

/** Logical window size for `root`'s current layout: ceil(rect + slack), floored at 1. */
export function measureWindowContentSize(root: HTMLElement): { width: number; height: number } {
	const rect = root.getBoundingClientRect();
	return {
		width: Math.max(1, Math.ceil(rect.width + WINDOW_SIZE_SLACK)),
		height: Math.max(1, Math.ceil(rect.height + WINDOW_SIZE_SLACK)),
	};
}

export interface WindowSizeSync {
	/** Request a size sync on the next animation frame; repeat calls coalesce. */
	schedule(): void;
	/** Cancel a pending sync (teardown). */
	cancel(): void;
}

/**
 * Build the size-sync loop over a `root` accessor (`null` while the element
 * is unmounted; both `schedule()` and the sync itself no-op then). The
 * optional `afterSync` hook runs after each scheduled sync settles, applied
 * or deduped, for work that must follow a possible window resize (e.g.
 * re-anchoring a satellite).
 */
export function createWindowSizeSync(
	root: () => HTMLElement | null,
	options: { afterSync?: () => void } = {},
): WindowSizeSync {
	let frame: number | null = null;
	let lastWidth: number | null = null;
	let lastHeight: number | null = null;

	async function sync(): Promise<void> {
		const el = root();
		if (!el) return;

		const { width, height } = measureWindowContentSize(el);
		if (width === lastWidth && height === lastHeight) return;

		lastWidth = width;
		lastHeight = height;

		try {
			await getCurrentWindow().setSize(new LogicalSize(width, height));
		} catch {
			lastWidth = null;
			lastHeight = null;
		}
	}

	return {
		schedule() {
			if (!root() || frame != null) return;
			frame = window.requestAnimationFrame(() => {
				frame = null;
				void sync().then(() => {
					options.afterSync?.();
				});
			});
		},
		cancel() {
			if (frame == null) return;
			window.cancelAnimationFrame(frame);
			frame = null;
		},
	};
}
