import { describe, expect, it, vi } from 'vitest';

import type { SessionQuestLinkSuggestion } from '$lib/api';
import { createPostSessionFlow, type PostSessionFlowOptions } from './postSession.svelte';

// The post-session state machine under test: the armour prompt gating the
// stop, the stop sequence's exact ordering (refresh, stats capture, stop,
// refresh), the quest-link suggestion's three outcomes, and the deferred
// clear while the armour-cost popup is open. Every dependency is injected,
// so the transitions run for real against controllable seams.

function suggestion(overrides: Partial<SessionQuestLinkSuggestion>): SessionQuestLinkSuggestion {
	return {
		sessionId: 's1',
		suggestionType: null,
		reason: null,
		questId: null,
		questName: null,
		playlistId: null,
		playlistName: null,
		...overrides,
	};
}

const stats = { cost: 12.5, returns: 11, pes: 0.4, net: -1.1 };

function makeFlow(overrides: Partial<PostSessionFlowOptions> = {}) {
	// `options` keeps the concrete mock types; tests overriding a dependency
	// assert on their own mock, not through `options`.
	const options = {
		isSessionActive: vi.fn(() => true),
		isBusy: vi.fn(() => false),
		armourReminderEnabled: vi.fn(() => false),
		refresh: vi.fn(async () => {}),
		readStats: vi.fn(() => ({ ...stats })),
		stopTracking: vi.fn(async () => ({ session_id: 's1' })),
		// Defaults to a linkable suggestion so the readout survives the
		// un-awaited suggestion load; the clear paths override this.
		fetchQuestLinkSuggestion: vi.fn(async () => suggestion({ suggestionType: 'quest' })),
		decideQuestLink: vi.fn(async () => {}),
		isArmourPopupOpen: vi.fn(() => false),
		showArmourPopup: vi.fn(async () => {}),
		onPromptShown: vi.fn(),
	};
	const flow = createPostSessionFlow({ ...options, ...overrides } satisfies PostSessionFlowOptions);
	return { flow, options };
}

/** Let the un-awaited suggestion load settle. */
async function flush(): Promise<void> {
	await Promise.resolve();
	await Promise.resolve();
	await Promise.resolve();
}

describe('requestStop', () => {
	it('does nothing when no session is active or a toggle is in flight', async () => {
		const inactive = makeFlow({ isSessionActive: vi.fn(() => false) });
		await inactive.flow.requestStop();
		expect(inactive.options.stopTracking).not.toHaveBeenCalled();
		expect(inactive.flow.awaitingArmourDecision).toBe(false);

		const busy = makeFlow({ isBusy: vi.fn(() => true) });
		await busy.flow.requestStop();
		expect(busy.options.stopTracking).not.toHaveBeenCalled();
	});

	it('arms the armour prompt instead of stopping when the reminder is enabled', async () => {
		const { flow, options } = makeFlow({ armourReminderEnabled: vi.fn(() => true) });

		await flow.requestStop();
		expect(flow.awaitingArmourDecision).toBe(true);
		expect(options.stopTracking).not.toHaveBeenCalled();
		expect(flow.stopping).toBe(false);
	});

	it('stops straight away, without the armour popup, when the reminder is disabled', async () => {
		const { flow, options } = makeFlow();

		await flow.requestStop();
		expect(options.stopTracking).toHaveBeenCalledTimes(1);
		expect(options.showArmourPopup).not.toHaveBeenCalled();
		expect(flow.lastSessionId).toBe('s1');
	});
});

describe('the stop sequence', () => {
	it('refreshes, captures the final stats, stops, then refreshes again, in that order', async () => {
		const { flow, options } = makeFlow();

		await flow.requestStop();

		expect(options.refresh).toHaveBeenCalledTimes(2);
		expect(flow.lastSessionStats).toEqual(stats);
		const order = (mock: { mock: { invocationCallOrder: number[] } }, call = 0) =>
			mock.mock.invocationCallOrder[call];
		// The pre-stop refresh lands before the stats capture (the readout must
		// show the true final totals, not a stale frame), which lands before the
		// stop; the second refresh follows the stop.
		expect(order(options.refresh)).toBeLessThan(order(options.readStats));
		expect(order(options.readStats)).toBeLessThan(order(options.stopTracking));
		expect(order(options.stopTracking)).toBeLessThan(order(options.refresh, 1));
	});

	it('skips the pre-stop refresh and stats when the session already went inactive', async () => {
		// The armour prompt can outlive the session (the backend may end it
		// while the prompt waits); answering still stops, but there is no live
		// readout to capture and no popup to show.
		const isSessionActive = vi.fn(() => true);
		const { flow, options } = makeFlow({
			isSessionActive,
			armourReminderEnabled: vi.fn(() => true),
		});
		await flow.requestStop();
		isSessionActive.mockReturnValue(false);

		await flow.decideArmourTrack('yes');
		expect(options.stopTracking).toHaveBeenCalledTimes(1);
		expect(options.refresh).toHaveBeenCalledTimes(1); // only the post-stop refresh
		expect(flow.lastSessionStats).toBeNull();
		expect(options.showArmourPopup).not.toHaveBeenCalled();
		await flush();
		expect(options.fetchQuestLinkSuggestion).toHaveBeenCalledWith('s1');
	});

	it('swallows a stop failure: stats stay captured, no session id, no suggestion load', async () => {
		const { flow, options } = makeFlow({
			stopTracking: vi.fn(async () => {
				throw new Error('backend away');
			}),
		});

		await flow.requestStop();
		await flush();
		expect(flow.stopping).toBe(false);
		expect(flow.lastSessionId).toBeNull();
		expect(flow.lastSessionStats).toEqual(stats);
		expect(options.fetchQuestLinkSuggestion).not.toHaveBeenCalled();
	});

	it('flags stopping for the duration of the stop', async () => {
		let resolveStop!: (value: { session_id: string }) => void;
		const { flow } = makeFlow({
			stopTracking: vi.fn(
				() =>
					new Promise<{ session_id: string }>((resolve) => {
						resolveStop = resolve;
					}),
			),
		});

		const pending = flow.requestStop();
		await flush();
		expect(flow.stopping).toBe(true);
		resolveStop({ session_id: 's1' });
		await pending;
		expect(flow.stopping).toBe(false);
	});
});

describe('decideArmourTrack', () => {
	it('does nothing when the prompt is not armed', async () => {
		const { flow, options } = makeFlow();
		await flow.decideArmourTrack('yes');
		expect(options.stopTracking).not.toHaveBeenCalled();
	});

	it('yes disarms the prompt, stops, then opens the armour popup after the stop', async () => {
		const { flow, options } = makeFlow({ armourReminderEnabled: vi.fn(() => true) });
		await flow.requestStop();

		await flow.decideArmourTrack('yes');
		expect(flow.awaitingArmourDecision).toBe(false);
		expect(options.stopTracking).toHaveBeenCalledTimes(1);
		expect(options.showArmourPopup).toHaveBeenCalledTimes(1);
		expect(options.showArmourPopup.mock.invocationCallOrder[0]).toBeGreaterThan(
			options.stopTracking.mock.invocationCallOrder[0],
		);
	});

	it('no stops without the armour popup', async () => {
		const { flow, options } = makeFlow({ armourReminderEnabled: vi.fn(() => true) });
		await flow.requestStop();

		await flow.decideArmourTrack('no');
		expect(options.stopTracking).toHaveBeenCalledTimes(1);
		expect(options.showArmourPopup).not.toHaveBeenCalled();
	});
});

describe('the quest-link suggestion', () => {
	it('shows a quest or playlist suggestion and keeps the readout', async () => {
		const linked = suggestion({ suggestionType: 'quest', questId: 'q1', questName: 'Daily' });
		const { flow, options } = makeFlow({
			fetchQuestLinkSuggestion: vi.fn(async () => linked),
		});

		await flow.requestStop();
		await flush();
		expect(flow.questLinkSuggestion).toEqual(linked);
		expect(flow.questLinkMessage).toBeNull();
		expect(flow.lastSessionId).toBe('s1');
		expect(flow.lastSessionStats).toEqual(stats);
		expect(options.onPromptShown).toHaveBeenCalledTimes(1);
	});

	it('shows the skip notice for an unclean or ambiguous record', async () => {
		for (const reason of ['unclean', 'ambiguous_playlist'] as const) {
			const { flow, options } = makeFlow({
				fetchQuestLinkSuggestion: vi.fn(async () => suggestion({ reason })),
			});
			await flow.requestStop();
			await flush();
			expect(flow.questLinkMessage).toBe('Unclean quest record, skipping linkage');
			expect(flow.questLinkSuggestion).toBeNull();
			expect(options.onPromptShown).toHaveBeenCalledTimes(1);
		}
	});

	it('clears the whole readout when there is nothing to suggest', async () => {
		const { flow } = makeFlow({
			fetchQuestLinkSuggestion: vi.fn(async () => suggestion({ reason: 'no_completions' })),
		});

		await flow.requestStop();
		await flush();
		expect(flow.lastSessionId).toBeNull();
		expect(flow.lastSessionStats).toBeNull();
	});

	it('clears when the suggestion fetch fails', async () => {
		const { flow } = makeFlow({
			fetchQuestLinkSuggestion: vi.fn(async () => {
				throw new Error('backend away');
			}),
		});

		await flow.requestStop();
		await flush();
		expect(flow.lastSessionId).toBeNull();
	});

	it('defers the clear while the armour popup is open, until it closes', async () => {
		const isArmourPopupOpen = vi.fn(() => true);
		const { flow } = makeFlow({
			isArmourPopupOpen,
			fetchQuestLinkSuggestion: vi.fn(async () => suggestion({ reason: 'no_completions' })),
		});

		await flow.requestStop();
		await flush();
		// Nothing to suggest, but the popup is open: the readout must survive.
		expect(flow.lastSessionId).toBe('s1');

		isArmourPopupOpen.mockReturnValue(false);
		flow.notifyArmourPopupClosed();
		expect(flow.lastSessionId).toBeNull();
		expect(flow.lastSessionStats).toBeNull();
	});

	it('a popup close without a deferred clear leaves a visible prompt alone', async () => {
		const { flow } = makeFlow({
			fetchQuestLinkSuggestion: vi.fn(async () => suggestion({ suggestionType: 'playlist' })),
		});

		await flow.requestStop();
		await flush();
		flow.notifyArmourPopupClosed();
		expect(flow.questLinkSuggestion).not.toBeNull();
		expect(flow.lastSessionId).toBe('s1');
	});
});

describe('decideQuestLink and dismiss', () => {
	it('records the verdict against the stopped session and ends the flow', async () => {
		const { flow, options } = makeFlow({
			fetchQuestLinkSuggestion: vi.fn(async () => suggestion({ suggestionType: 'quest' })),
		});
		await flow.requestStop();
		await flush();

		await flow.decideQuestLink('accept');
		expect(options.decideQuestLink).toHaveBeenCalledWith('s1', 'accept');
		expect(flow.lastSessionId).toBeNull();
		expect(flow.questLinkSuggestion).toBeNull();
		expect(flow.questLinkSaving).toBe(false);
	});

	it('still ends the flow when the decision write fails', async () => {
		const { flow } = makeFlow({
			fetchQuestLinkSuggestion: vi.fn(async () => suggestion({ suggestionType: 'quest' })),
			decideQuestLink: vi.fn(async () => {
				throw new Error('backend away');
			}),
		});
		await flow.requestStop();
		await flush();

		await flow.decideQuestLink('decline');
		expect(flow.lastSessionId).toBeNull();
	});

	it('does nothing without a stopped session', async () => {
		const { flow, options } = makeFlow();
		await flow.decideQuestLink('accept');
		expect(options.decideQuestLink).not.toHaveBeenCalled();
	});

	it('dismissing the skip notice ends the flow', async () => {
		const { flow } = makeFlow({
			fetchQuestLinkSuggestion: vi.fn(async () => suggestion({ reason: 'unclean' })),
		});
		await flow.requestStop();
		await flush();
		expect(flow.questLinkMessage).not.toBeNull();

		flow.dismissQuestLinkMessage();
		expect(flow.questLinkMessage).toBeNull();
		expect(flow.lastSessionId).toBeNull();
	});
});
