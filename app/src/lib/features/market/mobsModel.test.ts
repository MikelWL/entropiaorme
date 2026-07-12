import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { MarketMobRankingRow } from '$lib/api';
import { coveragePct, createMobsModel } from './mobsModel.svelte';

vi.mock('$lib/api', () => ({
	getMarketMobRanking: vi.fn(),
}));

import * as api from '$lib/api';

const mocked = vi.mocked(api);

function row(overrides: Partial<MarketMobRankingRow> = {}): MarketMobRankingRow {
	return {
		mobSpecies: 'Carabok',
		lootTt: 200,
		coveredTt: 100,
		itemCount: 2,
		coveredItemCount: 1,
		estMarkupPct: 106.88,
		...overrides,
	};
}

beforeEach(() => {
	vi.clearAllMocks();
});

describe('createMobsModel', () => {
	it('loads the ranking on the default week window', async () => {
		mocked.getMarketMobRanking.mockResolvedValue([row()]);
		const model = createMobsModel();
		await model.loadData();
		expect(mocked.getMarketMobRanking).toHaveBeenCalledWith('week');
		expect(model.rows).toHaveLength(1);
	});

	it('switching the window reloads', async () => {
		mocked.getMarketMobRanking.mockResolvedValue([row()]);
		const model = createMobsModel();
		await model.loadData();
		model.selectHorizon('month');
		await vi.waitFor(() => expect(mocked.getMarketMobRanking).toHaveBeenLastCalledWith('month'));
	});

	it('surfaces a load failure', async () => {
		mocked.getMarketMobRanking.mockRejectedValue(new Error('boom'));
		const model = createMobsModel();
		await model.loadData();
		expect(model.error).not.toBeNull();
	});
});

describe('coveragePct', () => {
	it('reads coverage as a whole percent, zero-safe', () => {
		expect(coveragePct(row())).toBe(50);
		expect(coveragePct(row({ coveredTt: 200 }))).toBe(100);
		expect(coveragePct(row({ lootTt: 0, coveredTt: 0 }))).toBe(0);
	});
});
