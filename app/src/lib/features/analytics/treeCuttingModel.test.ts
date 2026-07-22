import { beforeEach, describe, expect, it, vi } from 'vitest';
import type {
	AnalyticsHarvest,
	HarvestLootItem,
	MarketToolRankingRow,
} from '$lib/api/commands.gen';
import { createTreeCuttingModel, primaryTree } from './treeCuttingModel.svelte';

vi.mock('$lib/api', () => ({
	getAnalyticsHarvest: vi.fn(),
	getMarketToolRanking: vi.fn(),
}));

import * as api from '$lib/api';

const mocked = vi.mocked(api);

function item(name: string, quantity: number, valuePed: number): HarvestLootItem {
	return { itemName: name, quantity, valuePed };
}

function harvest(): AnalyticsHarvest {
	return {
		toolComparisons: [
			{
				toolName: 'Terratech PH-1 (L)',
				swings: 4562,
				cycled: 91.24,
				returns: 91.38,
				lootRate: 1.0015,
				lootItems: [
					item('Long Moonleaf Board', 120, 60.0),
					item('Wood Shavings', 800, 31.38),
				],
			},
			{
				toolName: 'Terratech PH-3',
				swings: 969,
				cycled: 96.9,
				returns: 94.33,
				lootRate: 0.9735,
				lootItems: [item('Short Moonleaf Board', 40, 94.33)],
			},
		],
	};
}

function marketRanking(): MarketToolRankingRow[] {
	return [
		{
			toolName: 'Terratech PH-1 (L)',
			lootTt: 91.38,
			coveredTt: 91.38,
			// 60 * 3.50 + 31.38 * 1.10 = 210 + 34.518 = 244.52 (rounded)
			muProjectedReturns: 244.52,
			items: [
				{ itemName: 'Long Moonleaf Board', markupPct: 350.0, horizon: 'week' },
				{ itemName: 'Wood Shavings', markupPct: 110.0, horizon: 'month' },
			],
		},
	];
}

beforeEach(() => {
	vi.clearAllMocks();
	mocked.getMarketToolRanking.mockResolvedValue([]);
});

describe('primaryTree', () => {
	it('maps the dominant board type to its tree size', () => {
		expect(primaryTree([item('Long Moonleaf Board', 1, 5), item('Wood Shavings', 1, 2)])).toBe(
			'Huge',
		);
		expect(primaryTree([item('Short Moonleaf Board', 1, 5)])).toBe('Small');
		expect(primaryTree([item('Moonleaf Board', 1, 5)])).toBe('Long');
	});

	it('picks the highest-TT board when several are present', () => {
		expect(
			primaryTree([
				item('Short Moonleaf Board', 1, 2),
				item('Long Moonleaf Board', 1, 9),
				item('Moonleaf Board', 1, 4),
			]),
		).toBe('Huge');
	});

	it('is null when no board loot has been recorded', () => {
		expect(primaryTree([item('Wood Shavings', 1, 5)])).toBeNull();
		expect(primaryTree([])).toBeNull();
	});
});

describe('loadData', () => {
	it('loads the harvest data', async () => {
		mocked.getAnalyticsHarvest.mockResolvedValue(harvest());
		const model = createTreeCuttingModel();
		await model.loadData();

		expect(model.data?.toolComparisons).toHaveLength(2);
		expect(model.loading).toBe(false);
		expect(model.error).toBeNull();
	});

	it('surfaces a load failure', async () => {
		mocked.getAnalyticsHarvest.mockRejectedValue(new Error('backend unreachable'));
		const model = createTreeCuttingModel();
		await model.loadData();
		expect(model.error).toBe('backend unreachable');
		expect(model.data).toBeNull();
	});
});

describe('sections', () => {
	it('builds a section per tool with the inferred tree and item shares', async () => {
		mocked.getAnalyticsHarvest.mockResolvedValue(harvest());
		const model = createTreeCuttingModel();
		await model.loadData();

		expect(model.sections.map((s) => s.toolName)).toEqual([
			'Terratech PH-1 (L)',
			'Terratech PH-3',
		]);

		const ph1 = model.sections[0];
		expect(ph1.tree).toBe('Huge');
		expect(ph1.returns).toBe(91.38);
		// Shares over the tool's own loot TT (60 + 31.38 = 91.38).
		expect(ph1.items[0].name).toBe('Long Moonleaf Board');
		expect(ph1.items[0].sharePct).toBeCloseTo((60 / 91.38) * 100, 4);
		expect(ph1.items[1].sharePct).toBeCloseTo((31.38 / 91.38) * 100, 4);

		expect(model.sections[1].tree).toBe('Small');
	});

	it('is empty before any data loads', () => {
		const model = createTreeCuttingModel();
		expect(model.sections).toEqual([]);
	});

	it('merges the market feed into MU figures and per-item markup', async () => {
		mocked.getAnalyticsHarvest.mockResolvedValue(harvest());
		mocked.getMarketToolRanking.mockResolvedValue(marketRanking());
		const model = createTreeCuttingModel();
		await model.loadData();

		const ph1 = model.sections[0];
		expect(ph1.muProjectedReturns).toBe(244.52);
		// MU rate = projected / cycled.
		expect(ph1.muRate).toBeCloseTo(244.52 / 91.24, 6);
		expect(ph1.coverage).toBeCloseTo(1, 6);
		expect(ph1.items[0].markupPct).toBe(350.0);
		expect(ph1.items[0].markupHorizon).toBe('week');
		expect(ph1.items[1].markupPct).toBe(110.0);

		// PH-3 has no market row: MU figures are null, markup uncovered.
		const ph3 = model.sections[1];
		expect(ph3.muProjectedReturns).toBeNull();
		expect(ph3.muRate).toBeNull();
		expect(ph3.coverage).toBeNull();
		expect(ph3.items[0].markupPct).toBeNull();
	});

	it('degrades to the realised view when the market feed fails', async () => {
		mocked.getAnalyticsHarvest.mockResolvedValue(harvest());
		mocked.getMarketToolRanking.mockRejectedValue(new Error('market offline'));
		const model = createTreeCuttingModel();
		await model.loadData();

		expect(model.error).toBeNull();
		expect(model.sections).toHaveLength(2);
		expect(model.sections[0].muProjectedReturns).toBeNull();
		expect(model.sections[0].returns).toBe(91.38);
	});
});
