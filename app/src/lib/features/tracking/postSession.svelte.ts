/**
 * Post-session flow for the tracking overlay: everything that happens between
 * the user asking to stop and the post-session readout clearing.
 *
 * One prompt: when the end-of-session armour reminder is enabled, the stop
 * request does not stop: it arms a Record protection? decision and the actual stop
 * runs only on the Record/Later answer. Record also opens the armour-cost popup after
 * the stop attempt settles (even a failed stop opens it: the popup anchors to
 * the still-current session); Later (or the reminder being disabled wholesale)
 * suppresses it.
 *
 * The stop itself re-reads the snapshot BEFORE capturing the final stats
 * (they are confirmed-ledger PED figures, so the readout must show the
 * session's true totals, not a stale frame), then stops, then re-reads again
 * for the idle state.
 *
 * The readout (session id + final stats) clears once the stop settles; while
 * the armour-cost popup is open the clear is deferred until the popup closes
 * (`notifyArmourPopupClosed`) so the readout does not vanish underneath it.
 * (The post-stop quest-link prompt this flow used to run retired with the
 * curated link model: the quest lifecycle records its own stretches now, so
 * there is nothing left to ask after the stop.)
 */

export interface PostSessionStats {
	cost: number;
	returns: number;
	pes: number;
	net: number;
}

export interface PostSessionFlowOptions {
	/** Whether a session is currently active (gates the stop request and the stats capture). */
	isSessionActive(): boolean;
	/** Whether a start/stop toggle is already in flight (the stop request defers to it). */
	isBusy(): boolean;
	/** Whether the end-of-session armour reminder is enabled (arms the armour prompt). */
	armourReminderEnabled(): boolean;
	/** Re-read the tracking snapshot (before the stats capture and after the stop). */
	refresh(): Promise<void>;
	/** The current session totals, read after `refresh()` settles. */
	readStats(): PostSessionStats;
	/** Stop the session; resolves with the stopped session's id. */
	stopTracking(): Promise<{ session_id: string }>;
	/** Whether the armour-cost popup is open (defers the readout clear). */
	isArmourPopupOpen(): boolean;
	/** Open the armour-cost popup (the Yes branch, after the stop settles). */
	showArmourPopup(): Promise<void>;
	/** A prompt or notice just became visible (hosts re-anchor satellites here). */
	onPromptShown?(): void;
}

export interface PostSessionFlow {
	/** The stopped session's id, until the flow clears. */
	readonly lastSessionId: string | null;
	/** The stopped session's final totals, until the flow clears. */
	readonly lastSessionStats: PostSessionStats | null;
	/** The armour prompt is showing (the stop is parked on its answer). */
	readonly awaitingArmourDecision: boolean;
	/** A stop sequence is in flight. */
	readonly stopping: boolean;
	/** Ask to stop: arms the armour prompt when the reminder is on, else stops. */
	requestStop(): Promise<void>;
	/** Answer the protection prompt; Record opens the cost popup after the stop. */
	decideArmourTrack(action: 'yes' | 'no'): Promise<void>;
	/** The armour-cost popup closed: run a clear that was deferred on it. */
	notifyArmourPopupClosed(): void;
}

export function createPostSessionFlow(options: PostSessionFlowOptions): PostSessionFlow {
	let lastSessionId = $state<string | null>(null);
	let lastSessionStats = $state<PostSessionStats | null>(null);
	let awaitingArmourDecision = $state(false);
	let stopping = $state(false);
	let clearPending = false;

	function clear(): void {
		clearPending = false;
		lastSessionId = null;
		lastSessionStats = null;
	}

	function clearWhenReady(): void {
		if (options.isArmourPopupOpen()) {
			clearPending = true;
			return;
		}
		clear();
	}

	async function stop(showArmour: boolean): Promise<void> {
		stopping = true;
		const wasActive = options.isSessionActive();
		try {
			// Refresh to the latest totals before capturing the final readout;
			// the session is still active here, so this reads the live totals
			// and stopping adds nothing to them.
			if (wasActive) await options.refresh();
			lastSessionStats = wasActive ? options.readStats() : null;

			const result = await options.stopTracking();
			lastSessionId = result.session_id;
			await options.refresh();
		} catch {
			/* ignore */
		}
		stopping = false;

		// The armour-cost popup is opt-in via the prompt's Yes branch;
		// suppressed when the user picked No or the reminder is disabled.
		if (wasActive && showArmour) {
			await options.showArmourPopup();
		}

		clearWhenReady();
	}

	return {
		get lastSessionId() {
			return lastSessionId;
		},
		get lastSessionStats() {
			return lastSessionStats;
		},
		get awaitingArmourDecision() {
			return awaitingArmourDecision;
		},
		get stopping() {
			return stopping;
		},
		async requestStop() {
			if (!options.isSessionActive() || options.isBusy()) return;
			if (options.armourReminderEnabled()) {
				awaitingArmourDecision = true;
				return;
			}
			await stop(false);
		},
		async decideArmourTrack(action: 'yes' | 'no') {
			if (!awaitingArmourDecision) return;
			awaitingArmourDecision = false;
			await stop(action === 'yes');
		},
		notifyArmourPopupClosed() {
			if (!clearPending) return;
			clear();
		},
	};
}
