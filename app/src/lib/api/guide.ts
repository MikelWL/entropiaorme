/**
 * Guide-mode read swap for analytics-flavoured surfaces.
 *
 * When the interactive user guide is active on an analytics-backed
 * surface, reads of analytics / tracking / ledger / inventory are
 * transparently retargeted onto the parallel typed `demo_*` commands
 * served by the curated demo database. Surface components stay
 * unchanged; only read wrappers branch, per call (never at client
 * construction), and everything else (live tracking, mutating verbs)
 * reaches the real backend regardless of guide state.
 */

import { guideState } from '$lib/guide/state.svelte';

/** A read wrapper that answers from the demo command while the guide is
 * active and from the live command otherwise, deciding per call. */
export function guideSwapped<Args extends unknown[], T>(
	live: (...args: Args) => Promise<T>,
	demo: (...args: Args) => Promise<T>,
): (...args: Args) => Promise<T> {
	return (...args: Args) => (guideState.isActive ? demo(...args) : live(...args));
}
