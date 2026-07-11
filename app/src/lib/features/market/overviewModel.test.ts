import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { MarketOverviewRow } from '$lib/api';
import { ageDays, createOverviewModel, formatAge, formatSalesPed } from './overviewModel.svelte';

vi.mock('$lib/api', () => ({
	getMarketOverview: vi.fn(),
}));

import * as api from '$lib/api';

const mocked = vi.mocked(api);

function row(overrides: Partial<MarketOverviewRow> = {}): MarketOverviewRow {
	const reading = (markupPct: number | null, salesPed: number) => ({ markupPct, salesPed });
	return {
		itemName: 'Carabok Hide',
		tier: 0,
		observedAt: 1_752_000_000,
		day: reading(106.88, 451.9),
		week: reading(107.16, 531.9),
		month: reading(106.02, 979.04),
		year: reading(108.28, 13_500),
		decade: reading(158.92, 35_300),
		...overrides,
	};
}

beforeEach(() => {
	vi.clearAllMocks();
});

describe('createOverviewModel', () => {
	it('loads rows and flattens the selected horizon (week by default)', async () => {
		mocked.getMarketOverview.mockResolvedValue([row()]);
		const model = createOverviewModel();
		await model.loadData();

		expect(model.loading).toBe(false);
		expect(model.horizon).toBe('week');
		expect(model.tableRows).toHaveLength(1);
		expect(model.tableRows[0].markupPct).toBe(107.16);
		expect(model.tableRows[0].salesPed).toBe(531.9);
	});

	it('switching the horizon re-derives without a reload', async () => {
		mocked.getMarketOverview.mockResolvedValue([row()]);
		const model = createOverviewModel();
		await model.loadData();

		model.horizon = 'decade';
		expect(model.tableRows[0].markupPct).toBe(158.92);
		expect(model.tableRows[0].salesPed).toBe(35_300);
		expect(mocked.getMarketOverview).toHaveBeenCalledTimes(1);
	});

	it('carries null markup through (N/A is never zero)', async () => {
		mocked.getMarketOverview.mockResolvedValue([row({ day: { markupPct: null, salesPed: 0 } })]);
		const model = createOverviewModel();
		await model.loadData();
		model.horizon = 'day';
		expect(model.tableRows[0].markupPct).toBeNull();
	});

	it('sorts null markups last in both directions and filters by search', async () => {
		mocked.getMarketOverview.mockResolvedValue([
			row({ itemName: 'Animal Oil Residue', week: { markupPct: 100.54, salesPed: 6_400 } }),
			row({ itemName: 'Carabok Leg Fur', week: { markupPct: null, salesPed: 0 } }),
			row({ itemName: 'Carabok Hide', week: { markupPct: 107.16, salesPed: 531.9 } }),
		]);
		const model = createOverviewModel();
		await model.loadData();

		model.sortKey = 'markupPct';
		model.sortDir = 'desc';
		expect(model.sortedRows.map((r) => r.itemName)).toEqual([
			'Carabok Hide',
			'Animal Oil Residue',
			'Carabok Leg Fur',
		]);
		model.sortDir = 'asc';
		expect(model.sortedRows.at(-1)?.itemName).toBe('Carabok Leg Fur');

		model.search = 'carabok';
		expect(model.sortedRows).toHaveLength(2);
	});

	it('surfaces a load failure', async () => {
		mocked.getMarketOverview.mockRejectedValue(new Error('boom'));
		const model = createOverviewModel();
		await model.loadData();
		expect(model.error).not.toBeNull();
		expect(model.tableRows).toHaveLength(0);
	});
});

describe('formatting helpers', () => {
	it('compacts sales volume', () => {
		expect(formatSalesPed(451.9)).toBe('451.90 PED');
		expect(formatSalesPed(13_500)).toBe('13.5K PED');
		expect(formatSalesPed(45_300_000)).toBe('45.3M PED');
	});

	it('labels observation age', () => {
		const now = 1_752_000_000;
		expect(ageDays(now, now)).toBe(0);
		expect(formatAge(now, now)).toBe('today');
		expect(formatAge(now - 3 * 86_400, now)).toBe('3d ago');
		expect(formatAge(now - 21 * 86_400, now)).toBe('3w ago');
	});
});
