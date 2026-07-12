import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { MarketHistoryPoint, MarketOverviewRow } from '$lib/api';
import { createHistoryModel } from './historyModel.svelte';

vi.mock('$lib/api', () => ({
	getMarketOverview: vi.fn(),
	getMarketItemHistory: vi.fn(),
}));

import * as api from '$lib/api';

const mocked = vi.mocked(api);

function overviewRow(itemName: string): MarketOverviewRow {
	const reading = { markupPct: 100, salesPed: 1 };
	return {
		itemName,
		tier: 0,
		observedAt: 1_752_000_000,
		day: reading,
		week: reading,
		month: reading,
		year: reading,
		decade: reading,
	};
}

function point(overrides: Partial<MarketHistoryPoint> = {}): MarketHistoryPoint {
	return { observedAt: 1_752_000_000, markupPct: 106.88, salesPed: 451.9, ...overrides };
}

beforeEach(() => {
	vi.clearAllMocks();
});

describe('createHistoryModel', () => {
	it('loads the item list, selects the first item, and loads its points', async () => {
		mocked.getMarketOverview.mockResolvedValue([
			overviewRow('Animal Muscle Oil'),
			overviewRow('Carabok Hide'),
		]);
		mocked.getMarketItemHistory.mockResolvedValue([point()]);
		const model = createHistoryModel();
		await model.loadItems();

		expect(model.itemNames).toEqual(['Animal Muscle Oil', 'Carabok Hide']);
		expect(model.selectedItem).toBe('Animal Muscle Oil');
		expect(mocked.getMarketItemHistory).toHaveBeenCalledWith('Animal Muscle Oil', 'week');
		expect(model.points).toHaveLength(1);
	});

	it('keeps a still-valid selection across a reload', async () => {
		mocked.getMarketOverview.mockResolvedValue([
			overviewRow('Animal Muscle Oil'),
			overviewRow('Carabok Hide'),
		]);
		mocked.getMarketItemHistory.mockResolvedValue([point()]);
		const model = createHistoryModel();
		await model.loadItems();
		model.selectItem('Carabok Hide');
		await vi.waitFor(() => expect(model.selectedItem).toBe('Carabok Hide'));

		await model.loadItems();
		expect(model.selectedItem).toBe('Carabok Hide');
	});

	it('switching horizon reloads the points for the selection', async () => {
		mocked.getMarketOverview.mockResolvedValue([overviewRow('Carabok Hide')]);
		mocked.getMarketItemHistory.mockResolvedValue([point()]);
		const model = createHistoryModel();
		await model.loadItems();

		model.selectHorizon('decade');
		await vi.waitFor(() =>
			expect(mocked.getMarketItemHistory).toHaveBeenLastCalledWith('Carabok Hide', 'decade'),
		);
	});

	it('an empty market yields no selection and no points', async () => {
		mocked.getMarketOverview.mockResolvedValue([]);
		const model = createHistoryModel();
		await model.loadItems();
		expect(model.selectedItem).toBeNull();
		expect(model.points).toHaveLength(0);
		expect(mocked.getMarketItemHistory).not.toHaveBeenCalled();
	});

	it('surfaces a history load failure', async () => {
		mocked.getMarketOverview.mockResolvedValue([overviewRow('Carabok Hide')]);
		mocked.getMarketItemHistory.mockRejectedValue(new Error('boom'));
		const model = createHistoryModel();
		await model.loadItems();
		expect(model.error).not.toBeNull();
	});
});
