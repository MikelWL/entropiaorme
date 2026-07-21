import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { AnalyticsHarvest } from '$lib/api/commands.gen';
import { createTreeCuttingModel } from './treeCuttingModel.svelte';

vi.mock('$lib/api', () => ({
	getAnalyticsHarvest: vi.fn(),
}));

import * as api from '$lib/api';

const mocked = vi.mocked(api);

function harvest(): AnalyticsHarvest {
	return {
		toolComparisons: [
			{ toolName: 'Terratech PH-1 (L)', swings: 4562, cycled: 91.24, lootRate: 1.0015 },
			{ toolName: 'Terratech PH-3', swings: 969, cycled: 96.9, lootRate: 0.9735 },
			{ toolName: 'Terratech PH-4 (L)', swings: 127, cycled: 111.13, lootRate: 0.6133 },
		],
	};
}

beforeEach(() => {
	vi.clearAllMocks();
});

describe('loadData', () => {
	it('loads the per-tool comparison table', async () => {
		mocked.getAnalyticsHarvest.mockResolvedValue(harvest());
		const model = createTreeCuttingModel();
		await model.loadData();

		expect(model.data?.toolComparisons).toHaveLength(3);
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

describe('sorted projection', () => {
	it('defaults to cycled descending and re-sorts on key or direction change', async () => {
		mocked.getAnalyticsHarvest.mockResolvedValue(harvest());
		const model = createTreeCuttingModel();
		await model.loadData();

		expect(model.toolSortKey).toBe('cycled');
		expect(model.toolSortDir).toBe('desc');
		expect(model.sortedTools.map((t) => t.toolName)).toEqual([
			'Terratech PH-4 (L)',
			'Terratech PH-3',
			'Terratech PH-1 (L)',
		]);

		model.toolSortKey = 'swings';
		expect(model.sortedTools.map((t) => t.swings)).toEqual([4562, 969, 127]);

		model.toolSortDir = 'asc';
		expect(model.sortedTools.map((t) => t.swings)).toEqual([127, 969, 4562]);
	});

	it('keeps the wire order untouched when no sort key is set', async () => {
		mocked.getAnalyticsHarvest.mockResolvedValue(harvest());
		const model = createTreeCuttingModel();
		await model.loadData();

		model.toolSortKey = undefined;
		expect(model.sortedTools.map((t) => t.toolName)).toEqual([
			'Terratech PH-1 (L)',
			'Terratech PH-3',
			'Terratech PH-4 (L)',
		]);
	});
});
