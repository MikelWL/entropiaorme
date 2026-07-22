import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { AnalyticsHarvest, HarvestLootItem } from '$lib/api/commands.gen';
import { createTreeCuttingModel, primaryTree } from './treeCuttingModel.svelte';

vi.mock('$lib/api', () => ({
	getAnalyticsHarvest: vi.fn(),
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

beforeEach(() => {
	vi.clearAllMocks();
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
});
