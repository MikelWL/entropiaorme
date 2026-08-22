import { describe, expect, it, vi } from 'vitest';

import { createPostSessionFlow, type PostSessionFlowOptions } from './postSession.svelte';

// The post-session state machine under test: the armour prompt gating the
// stop, the stop sequence's exact ordering (refresh, stats capture, stop,
// refresh), and the readout clear deferred while the armour-cost popup is
// open. Every dependency is injected, so the transitions run for real
// against controllable seams.

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
		captureArmourSetupOnLater: vi.fn(() => false),
		showArmourPopup: vi.fn(async () => true),
		showArmourWorkflowInSession: vi.fn(async () => true),
		onPromptShown: vi.fn(),
	};
	const flow = createPostSessionFlow({ ...options, ...overrides } satisfies PostSessionFlowOptions);
	return { flow, options };
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
		// Nothing holds the readout (no popup), so the flow ends cleared.
		expect(flow.lastSessionId).toBeNull();
	});
});

describe('the stop sequence', () => {
	it('refreshes, captures the final stats, stops, then refreshes again, in that order', async () => {
		const { flow, options } = makeFlow();

		await flow.requestStop();

		expect(options.refresh).toHaveBeenCalledTimes(2);
		expect(options.readStats).toHaveBeenCalledTimes(1);
		void flow;
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

		await flow.decideArmourTrack('no');
		expect(options.stopTracking).toHaveBeenCalledTimes(1);
		expect(options.refresh).toHaveBeenCalledTimes(1); // only the post-stop refresh
		expect(flow.lastSessionStats).toBeNull();
		expect(options.showArmourPopup).not.toHaveBeenCalled();
	});

	it('swallows a stop failure and still ends the flow cleared', async () => {
		const { flow } = makeFlow({
			stopTracking: vi.fn(async () => {
				throw new Error('backend away');
			}),
		});

		await flow.requestStop();
		expect(flow.stopping).toBe(false);
		expect(flow.lastSessionId).toBeNull();
		expect(flow.lastSessionStats).toBeNull();
	});

	it('does not open the armour workflow when the stop was refused', async () => {
		// The session is still running, so there is no ended session to record
		// against; the user retries the stop instead of meeting an armour prompt.
		const { flow, options } = makeFlow({
			stopTracking: vi.fn(async () => {
				throw new Error('backend away');
			}),
			captureArmourSetupOnLater: vi.fn(() => true),
		});

		await flow.requestStop();
		expect(options.showArmourPopup).not.toHaveBeenCalled();
		expect(flow.lastSessionId).toBeNull();
		expect(flow.lastSessionStats).toBeNull();
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
		await Promise.resolve();
		await Promise.resolve();
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

	it('Record opens the workflow against the session and leaves it running', async () => {
		// Armour cost belongs to the session it was spent in, so recording it
		// is part of that session rather than an afterthought about a closed
		// one. The user stops when they have finished.
		const { flow, options } = makeFlow({ armourReminderEnabled: vi.fn(() => true) });
		await flow.requestStop();

		await flow.decideArmourTrack('yes');
		expect(flow.awaitingArmourDecision).toBe(false);
		expect(options.showArmourWorkflowInSession).toHaveBeenCalledTimes(1);
		expect(options.stopTracking).not.toHaveBeenCalled();
		expect(options.showArmourPopup).not.toHaveBeenCalled();
		expect(flow.lastSessionId).toBeNull();
	});

	it('a second stop after recording still offers the prompt', async () => {
		const { flow, options } = makeFlow({ armourReminderEnabled: vi.fn(() => true) });
		await flow.requestStop();
		await flow.decideArmourTrack('yes');

		await flow.requestStop();
		expect(flow.awaitingArmourDecision).toBe(true);
		await flow.decideArmourTrack('no');
		expect(options.stopTracking).toHaveBeenCalledTimes(1);
	});

	it('no stops without the armour popup', async () => {
		const { flow, options } = makeFlow({ armourReminderEnabled: vi.fn(() => true) });
		await flow.requestStop();

		await flow.decideArmourTrack('no');
		expect(options.stopTracking).toHaveBeenCalledTimes(1);
		expect(options.showArmourPopup).not.toHaveBeenCalled();
	});

	it('Later captures only the whole-session setup when segment protection is disabled', async () => {
		const { flow, options } = makeFlow({
			armourReminderEnabled: vi.fn(() => true),
			captureArmourSetupOnLater: vi.fn(() => true),
		});
		await flow.requestStop();

		await flow.decideArmourTrack('no');
		expect(options.showArmourPopup).toHaveBeenCalledWith(false);
		expect(flow.lastSessionId).toBe('s1');
	});
});

describe('the deferred clear', () => {
	it('holds the readout while the armour popup is open, until it closes', async () => {
		const { flow } = makeFlow({
			armourReminderEnabled: vi.fn(() => true),
			captureArmourSetupOnLater: vi.fn(() => true),
		});

		await flow.requestStop();
		await flow.decideArmourTrack('no');
		// The popup is open: the readout must survive underneath it.
		expect(flow.lastSessionId).toBe('s1');
		expect(flow.lastSessionStats).toEqual(stats);

		flow.notifyArmourPopupClosed();
		expect(flow.lastSessionId).toBeNull();
		expect(flow.lastSessionStats).toBeNull();
	});

	it('a popup close without a deferred clear is a no-op', async () => {
		const { flow } = makeFlow();
		flow.notifyArmourPopupClosed();
		expect(flow.lastSessionId).toBeNull();
	});

	it('clears rather than stranding the readout when the popup fails to open', async () => {
		// A readout left standing with no workflow over it and nothing to
		// dismiss it wedges the overlay until the window is reloaded. The
		// session is offered back through the pending-attribution route, so
		// clearing loses nothing.
		const { flow } = makeFlow({
			armourReminderEnabled: vi.fn(() => true),
			captureArmourSetupOnLater: vi.fn(() => true),
			showArmourPopup: vi.fn(async () => false),
		});
		await flow.requestStop();
		await flow.decideArmourTrack('no');

		expect(flow.lastSessionId).toBeNull();
		expect(flow.lastSessionStats).toBeNull();
	});
});
