import { beforeEach, describe, expect, it, vi } from 'vitest';
import type {
	AnalyticsHarvest,
	HarvestLootItem,
	MarketHarvestData,
	MarketHarvestItem,
} from '$lib/api/commands.gen';
import {
	createTreeCuttingModel,
	effectiveMarkup,
	itemTier,
	NANOCUBE_FALLBACK_MARKUP,
	primaryTree,
} from './treeCuttingModel.svelte';

vi.mock('$lib/api', () => ({
	getAnalyticsHarvest: vi.fn(),
	getMarketHarvestMarkups: vi.fn(),
	getHarvestStock: vi.fn(),
	setHarvestStock: vi.fn(),
}));

import * as api from '$lib/api';

const mocked = vi.mocked(api);

// Sections are ordered by cycled volume, not data order, so tests select a
// section by its tool rather than a positional index.
const PH1 = 'Terratech PH-1 (L)'; // Long Moonleaf Board
const PH4 = 'Terratech PH-4 (L)'; // Wood Shavings
const sectionOf = (model: ReturnType<typeof createTreeCuttingModel>, tool: string) =>
	model.sections.find((s) => s.toolName === tool)!;

function loot(name: string, quantity: number, valuePed: number): HarvestLootItem {
	return { itemName: name, quantity, valuePed };
}

function obs(
	name: string,
	markupPct: number | null,
	horizon: string | null,
	salesPed: number | null,
	weeklyVolume: number | null = null,
): MarketHarvestItem {
	// Synthesise a day/week/month/year breakdown: the resolved horizon
	// carries (markup, salesPed), the week carries the weekly volume, the
	// rest are empty. Enough to exercise the resolved fields and the
	// weekly-sales signal derived from readings.
	const readings = ['day', 'week', 'month', 'year'].map((h) => ({
		horizon: h,
		markupPct: h === horizon ? markupPct : null,
		salesPed: h === horizon ? (salesPed ?? 0) : h === 'week' ? (weeklyVolume ?? 0) : 0,
	}));
	return { itemName: name, markupPct, horizon, salesPed, readings };
}

// Mirrors the maintainer's real tree-cutting data closely enough to
// exercise each tier.
function harvest(): AnalyticsHarvest {
	return {
		toolComparisons: [
			{
				toolName: 'Terratech PH-1 (L)',
				swings: 4562,
				cycled: 91.24,
				returns: 34.26,
				lootRate: 1.0015,
				lootItems: [loot('Long Moonleaf Board', 571, 34.26)],
			},
			{
				toolName: 'Terratech PH-4 (L)',
				swings: 127,
				cycled: 111.13,
				returns: 87.38,
				lootRate: 0.6133,
				lootItems: [loot('Wood Shavings', 87431, 87.38)],
			},
		],
	};
}

function market(): MarketHarvestData {
	return {
		nanocubeMarkupPct: 100.84,
		items: [
			// Liquid: weekly, position 34.26 is ~11% of 320 weekly volume.
			obs('Long Moonleaf Board', 353.69, 'week', 320.34, 320.34),
			// Illiquid: month fallback, position 87.38 exceeds weekly-equiv
			// (~84), and nothing sold in the last week.
			obs('Wood Shavings', 110.01, 'month', 363.61, 0),
		],
	};
}

beforeEach(() => {
	vi.clearAllMocks();
	mocked.getMarketHarvestMarkups.mockResolvedValue({ nanocubeMarkupPct: null, items: [] });
	mocked.getHarvestStock.mockResolvedValue([]);
	mocked.setHarvestStock.mockResolvedValue(undefined);
});

describe('primaryTree', () => {
	it('maps the dominant board type to its tree size', () => {
		expect(primaryTree([loot('Long Moonleaf Board', 1, 5)])).toBe('Huge');
		expect(primaryTree([loot('Short Moonleaf Board', 1, 5)])).toBe('Small');
		expect(primaryTree([loot('Moonleaf Board', 1, 5)])).toBe('Long');
		expect(primaryTree([loot('Wood Shavings', 1, 5)])).toBeNull();
	});
});

describe('itemTier', () => {
	it('is liquid when the position is a small share of weekly volume', () => {
		expect(itemTier(obs('X', 350, 'week', 320), 34)).toBe('liquid');
	});

	it('is middling when the position is a sizeable share of weekly volume', () => {
		// 73 / 184 = ~0.40 absorption, weekly horizon.
		expect(itemTier(obs('X', 110, 'week', 184), 73)).toBe('middling');
	});

	it('is illiquid when the position exceeds weekly throughput', () => {
		// Wood Shavings: month fallback, weekly-equiv ~84, position 87 -> >0.75.
		expect(itemTier(obs('X', 110, 'month', 363.61), 87.38)).toBe('illiquid');
	});

	it('is illiquid when uncovered or when the gain cannot clear the fee', () => {
		expect(itemTier(undefined, 100)).toBe('illiquid');
		expect(itemTier(obs('X', null, null, null), 100)).toBe('illiquid');
		// 5 PED position at 105% -> 0.25 PED gain < 0.5 fee.
		expect(itemTier(obs('X', 105, 'week', 10000), 5)).toBe('illiquid');
	});
});

describe('effectiveMarkup', () => {
	it('trusts the own markup when the tier clears the mode threshold', () => {
		expect(effectiveMarkup('liquid', 350, 100.84, 'liquid')).toEqual({
			markupPct: 350,
			floored: false,
		});
		expect(effectiveMarkup('middling', 110, 100.84, 'liquidMiddling')).toEqual({
			markupPct: 110,
			floored: false,
		});
	});

	it('floors to nanocube when the tier is below the mode threshold', () => {
		expect(effectiveMarkup('middling', 110, 100.84, 'liquid')).toEqual({
			markupPct: 100.84,
			floored: true,
		});
		expect(effectiveMarkup('illiquid', 110, 100.84, 'liquidMiddling')).toEqual({
			markupPct: 100.84,
			floored: true,
		});
	});

	it('shows own markup for any tier under the "all" mode', () => {
		expect(effectiveMarkup('illiquid', 110, 100.84, 'all')).toEqual({
			markupPct: 110,
			floored: false,
		});
	});

	it('always floors an uncovered item (no own markup to show)', () => {
		expect(effectiveMarkup('illiquid', null, 100.84, 'all')).toEqual({
			markupPct: 100.84,
			floored: true,
		});
	});
});

describe('loadData', () => {
	it('surfaces a load failure', async () => {
		mocked.getAnalyticsHarvest.mockRejectedValue(new Error('backend unreachable'));
		const model = createTreeCuttingModel();
		await model.loadData();
		expect(model.error).toBe('backend unreachable');
		expect(model.data).toBeNull();
	});
});

describe('sections', () => {
	it('assigns tiers and floors sub-threshold markups by mode', async () => {
		mocked.getAnalyticsHarvest.mockResolvedValue(harvest());
		mocked.getMarketHarvestMarkups.mockResolvedValue(market());
		const model = createTreeCuttingModel(); // default mode: liquidMiddling

		await model.loadData();

		const long = sectionOf(model, PH1).items[0];
		expect(long.tier).toBe('liquid');
		expect(long.floored).toBe(false);
		expect(long.effectiveMarkupPct).toBe(353.69);
		expect(long.salesPed).toBe(320.34);
		expect(long.weeklySalesPed).toBe(320.34);

		const wood = sectionOf(model, PH4).items[0];
		expect(wood.tier).toBe('illiquid');
		expect(wood.floored).toBe(true); // illiquid < liquidMiddling threshold
		expect(wood.effectiveMarkupPct).toBe(100.84); // nanocube floor
		// Un-normalised fallback volume, and zero weekly sales.
		expect(wood.salesPed).toBe(363.61);
		expect(wood.weeklySalesPed).toBe(0);
	});

	it('combines every tool into the overall aggregate', async () => {
		mocked.getAnalyticsHarvest.mockResolvedValue(harvest());
		mocked.getMarketHarvestMarkups.mockResolvedValue(market());
		const model = createTreeCuttingModel();
		await model.loadData();

		const overall = model.overall!;
		// Cycled and returns sum across both tools; rate is volume-weighted.
		expect(overall.cycled).toBeCloseTo(91.24 + 111.13, 4);
		expect(overall.returns).toBeCloseTo(34.26 + 87.38, 4);
		expect(overall.lootRate).toBeCloseTo((34.26 + 87.38) / (91.24 + 111.13), 4);
		// MU projected sums the per-section (mode-respecting) figures.
		expect(overall.muProjectedReturns).toBeCloseTo(
			model.sections[0].muProjectedReturns! + model.sections[1].muProjectedReturns!,
			4,
		);
	});

	it('drops overall market figures when the market feed is unavailable', async () => {
		mocked.getAnalyticsHarvest.mockResolvedValue(harvest());
		mocked.getMarketHarvestMarkups.mockResolvedValue(null as unknown as MarketHarvestData);
		const model = createTreeCuttingModel();
		await model.loadData();
		expect(model.overall!.muProjectedReturns).toBeNull();
		expect(model.overall!.muRate).toBeNull();
		expect(model.overall!.returns).toBeCloseTo(34.26 + 87.38, 4);
	});

	it('recomputes MU projected returns when the mode changes', async () => {
		mocked.getAnalyticsHarvest.mockResolvedValue(harvest());
		mocked.getMarketHarvestMarkups.mockResolvedValue(market());
		const model = createTreeCuttingModel();
		await model.loadData();

		// Wood Shavings section: illiquid item.
		// Default (liquidMiddling): floored to nanocube -> 87.38 * 1.0084.
		expect(sectionOf(model, PH4).muProjectedReturns).toBeCloseTo((87.38 * 100.84) / 100, 4);

		// "all": trusts the item's own 110.01% markup.
		model.confidenceMode = 'all';
		expect(sectionOf(model, PH4).muProjectedReturns).toBeCloseTo((87.38 * 110.01) / 100, 4);
	});

	it('uses the nanocube fallback constant when the feed lacks a nanocube row', async () => {
		mocked.getAnalyticsHarvest.mockResolvedValue(harvest());
		mocked.getMarketHarvestMarkups.mockResolvedValue({
			nanocubeMarkupPct: null,
			items: [obs('Wood Shavings', 110.01, 'month', 363.61)],
		});
		const model = createTreeCuttingModel();
		await model.loadData();
		// Wood Shavings floored to the constant.
		expect(model.sections[1].items[0].effectiveMarkupPct).toBe(NANOCUBE_FALLBACK_MARKUP);
	});

	it('degrades to the realised view when the market feed fails', async () => {
		mocked.getAnalyticsHarvest.mockResolvedValue(harvest());
		mocked.getMarketHarvestMarkups.mockResolvedValue(null as unknown as MarketHarvestData);
		const model = createTreeCuttingModel();
		await model.loadData();

		expect(model.error).toBeNull();
		expect(model.sections).toHaveLength(2);
		const long = sectionOf(model, PH1);
		expect(long.muProjectedReturns).toBeNull();
		expect(long.muRate).toBeNull();
		expect(long.returns).toBe(34.26);
	});

	it('orders sections by cycled volume and opens the busiest by default', async () => {
		mocked.getAnalyticsHarvest.mockResolvedValue(harvest());
		const model = createTreeCuttingModel();
		await model.loadData();

		// PH-4 cycled 111.13 outranks PH-1's 91.24, so it leads the list and
		// opens as the default selection.
		expect(model.sections.map((s) => s.toolName)).toEqual([PH4, PH1]);
		expect(model.selectedSection?.toolName).toBe(PH4);

		// Selecting another sub-activity swaps the open detail.
		model.selectSection(PH1);
		expect(model.selectedSection?.toolName).toBe(PH1);
	});
});

describe('stock', () => {
	it('defaults every recorded item to fully held', async () => {
		mocked.getAnalyticsHarvest.mockResolvedValue(harvest());
		const model = createTreeCuttingModel();
		await model.loadData();

		// Ordered by stock TT, most-held first: Wood Shavings (87.38) then
		// Long Moonleaf Board (34.26).
		expect(model.stock.map((s) => s.itemName)).toEqual(['Wood Shavings', 'Long Moonleaf Board']);
		const long = model.stock.find((s) => s.itemName === 'Long Moonleaf Board')!;
		expect(long.lootedQty).toBe(571);
		expect(long.removedQty).toBe(0);
		expect(long.heldQty).toBe(571);
		expect(long.heldTt).toBeCloseTo(34.26, 4);
	});

	it('reflects a removed overlay loaded from the backend', async () => {
		mocked.getAnalyticsHarvest.mockResolvedValue(harvest());
		mocked.getHarvestStock.mockResolvedValue([
			{ itemName: 'Long Moonleaf Board', removedQty: 71 },
		]);
		const model = createTreeCuttingModel();
		await model.loadData();

		const long = model.stock.find((s) => s.itemName === 'Long Moonleaf Board')!;
		expect(long.removedQty).toBe(71);
		expect(long.heldQty).toBe(500);
		// TT scales with the held fraction.
		expect(long.heldTt).toBeCloseTo((34.26 * 500) / 571, 4);
	});

	it('persists the derived removed quantity when held is edited', async () => {
		mocked.getAnalyticsHarvest.mockResolvedValue(harvest());
		const model = createTreeCuttingModel();
		await model.loadData();

		await model.setHeld('Long Moonleaf Board', 500);
		expect(mocked.setHarvestStock).toHaveBeenCalledWith({
			itemName: 'Long Moonleaf Board',
			removedQty: 71,
		});
		expect(model.stock.find((s) => s.itemName === 'Long Moonleaf Board')!.heldQty).toBe(500);
	});

	it('feeds markup confidence: selling down to a thin position turns illiquid', async () => {
		mocked.getAnalyticsHarvest.mockResolvedValue(harvest());
		mocked.getMarketHarvestMarkups.mockResolvedValue(market());
		const model = createTreeCuttingModel();
		await model.loadData();

		// Fully held, Long Moonleaf Board is liquid.
		expect(sectionOf(model, PH1).items[0].tier).toBe('liquid');

		// Sell almost everything: the tiny remaining position cannot clear
		// the auction fee, so the markup floors.
		await model.setHeld('Long Moonleaf Board', 3);
		expect(sectionOf(model, PH1).items[0].tier).toBe('illiquid');
		expect(sectionOf(model, PH1).items[0].floored).toBe(true);
	});

	it('joins each stock row to its resolved markup and horizon breakdown', async () => {
		mocked.getAnalyticsHarvest.mockResolvedValue(harvest());
		mocked.getMarketHarvestMarkups.mockResolvedValue(market());
		const model = createTreeCuttingModel();
		await model.loadData();

		const long = model.stock.find((s) => s.itemName === 'Long Moonleaf Board')!;
		expect(long.markupPct).toBe(353.69);
		expect(long.markupHorizon).toBe('week');
		// Ordered day, week, month, year, from the synthesised breakdown.
		expect(long.readings.map((r) => r.horizon)).toEqual(['day', 'week', 'month', 'year']);
		expect(long.readings.find((r) => r.horizon === 'week')?.markupPct).toBe(353.69);

		const wood = model.stock.find((s) => s.itemName === 'Wood Shavings')!;
		expect(wood.markupPct).toBe(110.01);
		expect(wood.markupHorizon).toBe('month');
	});

	it('clears the overlay row when held is set back to full', async () => {
		mocked.getAnalyticsHarvest.mockResolvedValue(harvest());
		const model = createTreeCuttingModel();
		await model.loadData();

		await model.setHeld('Long Moonleaf Board', 571);
		// removedQty 0 clears rather than stores.
		expect(mocked.setHarvestStock).toHaveBeenCalledWith({
			itemName: 'Long Moonleaf Board',
			removedQty: 0,
		});
		expect(model.stock.find((s) => s.itemName === 'Long Moonleaf Board')!.removedQty).toBe(0);
	});
});
