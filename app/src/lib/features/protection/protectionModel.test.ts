import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { ProtectionObservationOutcome, ProtectionOverview, ProtectionSet } from '$lib/api';
import { createProtectionModel } from './protectionModel.svelte';

vi.mock('$lib/api', () => ({
	archiveProtectionLoadout: vi.fn(),
	archiveProtectionSet: vi.fn(),
	confirmProtectionObservation: vi.fn(),
	createProtectionLoadout: vi.fn(),
	createProtectionSet: vi.fn(),
	getProtectionOverview: vi.fn(),
	scanTradeTerminalValue: vi.fn(),
	selectProtectionLoadout: vi.fn(),
	updateProtectionLoadout: vi.fn(),
	updateProtectionSet: vi.fn(),
}));

import * as api from '$lib/api';

const mocked = vi.mocked(api);

const set: ProtectionSet = {
	id: '7',
	kind: 'armour',
	name: 'Limited armour',
	economyKind: 'limited',
	markupPercent: 120,
	latestObservation: null,
	pendingReconciliations: 0,
	basisLocked: false,
	unsettledDamage: 0,
	unsettledDeflections: 0,
	unsettledSessions: 0,
};

const overview: ProtectionOverview = {
	sets: [set],
	loadouts: [],
	activeLoadoutId: null,
	recentReconciliations: [],
	recentCostWindows: [],
};

const outcome: ProtectionObservationOutcome = {
	observation: {
		id: '11',
		setId: set.id,
		ttValuePed: 10,
		source: 'manual',
		rawText: null,
		observedAt: 1,
		resetReason: null,
		defenceEventCursor: '0',
	},
	reconciliation: null,
	costWindow: null,
};

beforeEach(() => {
	vi.clearAllMocks();
	mocked.getProtectionOverview.mockResolvedValue(overview);
});

describe('protection observation idempotency', () => {
	it('reuses one token when an ambiguous write failure is retried', async () => {
		mocked.confirmProtectionObservation
			.mockRejectedValueOnce(new Error('connection lost'))
			.mockResolvedValueOnce(outcome);
		const model = createProtectionModel();
		model.openObservation(set);

		expect(await model.confirmObservation({ valuePed: 10, source: 'manual' })).toBeNull();
		expect(await model.confirmObservation({ valuePed: 10, source: 'manual' })).toBe(outcome);

		const first = mocked.confirmProtectionObservation.mock.calls[0][0].clientToken;
		const second = mocked.confirmProtectionObservation.mock.calls[1][0].clientToken;
		expect(second).toBe(first);
	});

	it('reports a committed observation even when the follow-up overview refresh fails', async () => {
		mocked.confirmProtectionObservation.mockResolvedValue(outcome);
		mocked.getProtectionOverview.mockRejectedValue(new Error('offline'));
		const model = createProtectionModel();
		model.openObservation(set);

		expect(await model.confirmObservation({ valuePed: 10, source: 'manual' })).toBe(outcome);
		expect(model.lastOutcome).toBe(outcome);
		expect(model.error).toContain('TT value recorded');
	});
});
