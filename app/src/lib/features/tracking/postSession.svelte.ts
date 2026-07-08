/**
 * Post-session flow for the tracking overlay: everything that happens between
 * the user asking to stop and the post-session readout clearing.
 *
 * Two prompts, in a fixed order:
 *
 * 1. **Armour prompt.** When the end-of-session armour reminder is enabled,
 *    the stop request does not stop: it arms a Track armour? decision and the
 *    actual stop runs only on the Yes/No answer. Yes also opens the
 *    armour-cost popup once the session has stopped; No (or the reminder
 *    being disabled wholesale) suppresses it.
 * 2. **Quest-link suggestion.** After the stop lands, the backend may suggest
 *    linking the session to a quest or playlist; the user accepts or
 *    declines. An unclean or ambiguous record shows a dismissable skip
 *    notice instead; any other verdict ends the flow silently.
 *
 * The stop itself re-reads the snapshot BEFORE capturing the final stats
 * (they are confirmed-ledger PED figures, so the readout must show the
 * session's true totals, not a stale frame), then stops, then re-reads again
 * for the idle state.
 *
 * The readout (session id + final stats) survives until the quest-link flow
 * resolves; when the armour-cost popup is open at that moment, the clear is
 * deferred until the popup closes (`notifyArmourPopupClosed`) so the readout
 * does not vanish underneath it.
 */

import type { SessionQuestLinkSuggestion } from '$lib/api';

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
	/** Fetch the quest-link suggestion for the stopped session. */
	fetchQuestLinkSuggestion(sessionId: string): Promise<SessionQuestLinkSuggestion>;
	/** Record the user's accept/decline verdict (the result is not consumed). */
	decideQuestLink(sessionId: string, action: 'accept' | 'decline'): Promise<unknown>;
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
	readonly questLinkSuggestion: SessionQuestLinkSuggestion | null;
	readonly questLinkMessage: string | null;
	readonly questLinkSaving: boolean;
	/** The armour prompt is showing (the stop is parked on its answer). */
	readonly awaitingArmourDecision: boolean;
	/** A stop sequence is in flight. */
	readonly stopping: boolean;
	/** Ask to stop: arms the armour prompt when the reminder is on, else stops. */
	requestStop(): Promise<void>;
	/** Answer the armour prompt; Yes opens the armour-cost popup after the stop. */
	decideArmourTrack(action: 'yes' | 'no'): Promise<void>;
	/** Answer the quest-link suggestion; always ends the flow. */
	decideQuestLink(action: 'accept' | 'decline'): Promise<void>;
	/** Dismiss the skip notice; ends the flow. */
	dismissQuestLinkMessage(): void;
	/** The armour-cost popup closed: run a clear that was deferred on it. */
	notifyArmourPopupClosed(): void;
}

export function createPostSessionFlow(options: PostSessionFlowOptions): PostSessionFlow {
	let lastSessionId = $state<string | null>(null);
	let lastSessionStats = $state<PostSessionStats | null>(null);
	let questLinkSuggestion = $state<SessionQuestLinkSuggestion | null>(null);
	let questLinkMessage = $state<string | null>(null);
	let questLinkSaving = $state(false);
	let awaitingArmourDecision = $state(false);
	let stopping = $state(false);
	let clearPending = false;

	function clear(): void {
		clearPending = false;
		lastSessionId = null;
		lastSessionStats = null;
		questLinkSuggestion = null;
		questLinkMessage = null;
		questLinkSaving = false;
	}

	function clearWhenReady(): void {
		if (options.isArmourPopupOpen()) {
			clearPending = true;
			return;
		}
		clear();
	}

	async function loadQuestLinkSuggestion(sessionId: string): Promise<void> {
		questLinkSuggestion = null;
		questLinkMessage = null;
		try {
			const suggestion = await options.fetchQuestLinkSuggestion(sessionId);
			if (suggestion.suggestionType === 'quest' || suggestion.suggestionType === 'playlist') {
				questLinkSuggestion = suggestion;
				options.onPromptShown?.();
				return;
			}
			if (suggestion.reason === 'unclean' || suggestion.reason === 'ambiguous_playlist') {
				questLinkMessage = 'Unclean quest record, skipping linkage';
				options.onPromptShown?.();
				return;
			}
		} catch {
			/* ignore */
		}
		clearWhenReady();
	}

	async function stop(showArmour: boolean): Promise<void> {
		stopping = true;
		const wasActive = options.isSessionActive();
		let stoppedSessionId: string | null = null;
		try {
			// Refresh to the latest totals before capturing the final readout;
			// the session is still active here, so this reads the live totals
			// and stopping adds nothing to them.
			if (wasActive) await options.refresh();
			lastSessionStats = wasActive ? options.readStats() : null;

			const result = await options.stopTracking();
			stoppedSessionId = result.session_id;
			lastSessionId = stoppedSessionId;
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

		if (stoppedSessionId) {
			void loadQuestLinkSuggestion(stoppedSessionId);
		}
	}

	return {
		get lastSessionId() {
			return lastSessionId;
		},
		get lastSessionStats() {
			return lastSessionStats;
		},
		get questLinkSuggestion() {
			return questLinkSuggestion;
		},
		get questLinkMessage() {
			return questLinkMessage;
		},
		get questLinkSaving() {
			return questLinkSaving;
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
		async decideQuestLink(action: 'accept' | 'decline') {
			if (!lastSessionId) return;
			questLinkSaving = true;
			try {
				await options.decideQuestLink(lastSessionId, action);
			} catch {
				/* ignore */
			}
			clear();
			questLinkSaving = false;
		},
		dismissQuestLinkMessage() {
			clear();
		},
		notifyArmourPopupClosed() {
			if (!clearPending) return;
			clear();
		},
	};
}
