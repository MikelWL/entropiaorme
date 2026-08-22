/**
 * Post-session flow for the tracking overlay: everything that happens between
 * the user asking to stop and the post-session readout clearing.
 *
 * One prompt: when the end-of-session armour reminder is enabled, the stop
 * request does not stop: it arms a Record armour costs? decision.
 *
 * Record does not stop either. Armour cost is part of the session it was spent
 * in, and a reading taken after the session ended reads like an afterthought
 * about something already closed, so Record opens the armour-cost workflow
 * against the session still running and leaves it running. The user stops when
 * they have finished, answering Later that time.
 *
 * Later stops. For a session that opted out of segment declarations it still
 * captures the whole-session setup, so a later reading has an identity to
 * reconcile against.
 *
 * The stop itself re-reads the snapshot BEFORE capturing the final stats
 * (they are confirmed-ledger PED figures, so the readout must show the
 * session's true totals, not a stale frame), then stops, then re-reads again
 * for the idle state.
 *
 * The readout (session id + final stats) clears once the stop settles, deferred
 * while the armour workflow is open so the stopped-session anchor does not
 * vanish underneath it. If that workflow fails to open, the readout clears
 * anyway rather than standing with nothing over it and nothing to dismiss it.
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
	/** Whether Later must still capture a whole-session protection setup. */
	captureArmourSetupOnLater(): boolean;
	/** Open the armour workflow and report only once the satellite is open. */
	showArmourPopup(recordNow: boolean): Promise<boolean>;
	/** Open the armour workflow against the session still running. */
	showArmourWorkflowInSession(): Promise<boolean>;
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
	/**
	 * Answer the armour prompt. Record opens the cost workflow and leaves the
	 * session running; Later stops it.
	 */
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

	async function stop(showArmour: boolean): Promise<void> {
		stopping = true;
		const wasActive = options.isSessionActive();
		let stopped = false;
		try {
			// Refresh to the latest totals before capturing the final readout;
			// the session is still active here, so this reads the live totals
			// and stopping adds nothing to them.
			if (wasActive) await options.refresh();
			lastSessionStats = wasActive ? options.readStats() : null;

			const result = await options.stopTracking();
			stopped = true;
			lastSessionId = result.session_id;
			await options.refresh();
		} catch {
			/* ignore */
		}
		stopping = false;

		// A refused stop leaves the session running; the armour workflow belongs
		// to a session that ended, so the user retries the stop instead.
		if (wasActive && !stopped) {
			clear();
			return;
		}

		// Recording cost is opt-in via the prompt's Record branch. An opted-out
		// session still needs its whole-session setup captured on Later.
		const openArmourWorkflow = wasActive && (showArmour || options.captureArmourSetupOnLater());
		if (openArmourWorkflow) {
			const opened = await options.showArmourPopup(showArmour);
			if (opened) {
				clearPending = true;
				return;
			}
			// The satellite did not open. The host surfaces its launch error;
			// what must not happen is the readout staying up with no workflow
			// over it and nothing left to dismiss it, which strands the overlay
			// in a stopped state until the window is reloaded. Clearing costs
			// nothing now that a session left unattributed is offered back
			// through the pending-attribution route.
			clear();
			return;
		}

		clear();
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
			if (action === 'yes') {
				// Recording belongs to the session, so the session stays
				// running and the stop is the user's next move, not this one's.
				await options.showArmourWorkflowInSession();
				return;
			}
			await stop(false);
		},
		notifyArmourPopupClosed() {
			if (!clearPending) return;
			clear();
		},
	};
}
