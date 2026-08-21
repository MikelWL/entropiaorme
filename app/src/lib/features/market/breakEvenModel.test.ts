import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { MarketBreakEven } from '$lib/api';
import { createBreakEvenModel } from './breakEvenModel.svelte';

vi.mock('$lib/api', () => ({
	getMarketBreakEven: vi.fn(),
}));

import * as api from '$lib/api';

const mocked = vi.mocked(api);

function payload(): MarketBreakEven {
	const loadout = (name: string, breakEvenLootMarkupPct: number | null) => ({
		name,
		amplifierName: null,
		weightedEfficiencyPct: breakEvenLootMarkupPct === null ? null : 70,
		offensiveTtRecoveryPct: breakEvenLootMarkupPct === null ? null : 95,
		expectedTtReturnPct: breakEvenLootMarkupPct === null ? null : 94,
		breakEvenLootMarkupPct,
		looterLevel: breakEvenLootMarkupPct === null ? null : 37.2,
		coverage: breakEvenLootMarkupPct === null ? null : 1,
		incomplete: breakEvenLootMarkupPct === null,
		modelVersion: breakEvenLootMarkupPct === null ? null : 'community_v1',
	});
	return {
		looters: [
			{ name: 'Animal Looter', level: 37.2 },
			{ name: 'Mutant Looter', level: 12.0 },
			{ name: 'Robot Looter', level: 24.5 },
		],
		weapons: [
			loadout('Unknown Blade', null),
			loadout('Low Efficiency Gun', 110.1),
			loadout('High Efficiency Gun', 105.6),
		],
	};
}

beforeEach(() => {
	vi.clearAllMocks();
});

describe('createBreakEvenModel', () => {
	it('sorts known weapons by best break-even and unknowns last', async () => {
		mocked.getMarketBreakEven.mockResolvedValue(payload());
		const model = createBreakEvenModel();
		await model.loadData();

		expect(model.looters.map((l) => l.name)).toEqual([
			'Animal Looter',
			'Mutant Looter',
			'Robot Looter',
		]);
		expect(model.weapons.map((w) => w.name)).toEqual([
			'High Efficiency Gun',
			'Low Efficiency Gun',
			'Unknown Blade',
		]);
	});

	it('surfaces a load failure', async () => {
		mocked.getMarketBreakEven.mockRejectedValue(new Error('boom'));
		const model = createBreakEvenModel();
		await model.loadData();
		expect(model.error).not.toBeNull();
		expect(model.weapons).toHaveLength(0);
	});
});
