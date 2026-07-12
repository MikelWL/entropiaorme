import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { MarketBreakEven } from '$lib/api';
import { createBreakEvenModel } from './breakEvenModel.svelte';

vi.mock('$lib/api', () => ({
	getMarketBreakEven: vi.fn(),
}));

import * as api from '$lib/api';

const mocked = vi.mocked(api);

function payload(): MarketBreakEven {
	const cell = (looterName: string, breakEvenMarkupPct: number) => ({
		looterName,
		ttReturnPct: 100 / (1 + breakEvenMarkupPct / 100),
		breakEvenMarkupPct,
	});
	return {
		looters: [
			{ name: 'Animal Looter', level: 37.2 },
			{ name: 'Mutant Looter', level: 12.0 },
		],
		weapons: [
			{ name: 'Unknown Blade', efficiencyPct: null, cells: [] },
			{
				name: 'Low Efficiency Gun',
				efficiencyPct: 55.0,
				cells: [cell('Animal Looter', 8.2), cell('Mutant Looter', 10.1)],
			},
			{
				name: 'High Efficiency Gun',
				efficiencyPct: 88.0,
				cells: [cell('Animal Looter', 5.6), cell('Mutant Looter', 7.4)],
			},
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

		expect(model.looters.map((l) => l.name)).toEqual(['Animal Looter', 'Mutant Looter']);
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
