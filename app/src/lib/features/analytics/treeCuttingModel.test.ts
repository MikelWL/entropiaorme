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
	marketOpportunity,
	NANOCUBE_FALLBACK_MARKUP,
	opportunityTier,
	primaryTree,
	weeklyEquivalentVolume,
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

function required<T>(value: T | null | undefined, label: string): T {
	if (value == null) throw new Error(`Expected ${label}`);
	return value;
}

const sectionOf = (model: ReturnType<typeof createTreeCuttingModel>, tool: string) =>
	required(
		model.sections.find((section) => section.toolName === tool),
		`section ${tool}`,
	);
const stockOf = (model: ReturnType<typeof createTreeCuttingModel>, itemName: string) =>
	required(
		model.stock.find((item) => item.itemName === itemName),
		`stock item ${itemName}`,
	);

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
// exercise broad and thin market opportunities.
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
			// Broad: strong premium with weekly turnover.
			obs('Long Moonleaf Board', 353.69, 'week', 320.34, 320.34),
			// Thin: modest premium, month fallback, and no weekly sales.
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

describe('marketOpportunity', () => {
	it('normalises supported horizons to weekly turnover', () => {
		expect(weeklyEquivalentVolume(434.5, 'month')).toBeCloseTo(100, 4);
		expect(weeklyEquivalentVolume(5214, 'year')).toBeCloseTo(100, 4);
		expect(weeklyEquivalentVolume(100, null)).toBe(0);
	});

	it('classifies a fee-efficient weekly market as broad', () => {
		const opportunity = marketOpportunity(obs('X', 100.8, 'week', 2_600_000), 100.6);
		expect(opportunity.kind).toBe('broad');
		expect(opportunity.usesNanocube).toBe(false);
		expect(opportunity.efficientBatchTt).toBeCloseTo(625, 4);
		expect(opportunity.weeklyPremiumThroughput).toBeCloseTo(20_800, 4);
	});

	it('preserves a sparse high-margin market as niche', () => {
		const opportunity = marketOpportunity(obs('X', 3000, 'month', 20), 100.6);
		expect(opportunity.kind).toBe('niche');
		expect(opportunity.usesNanocube).toBe(false);
		expect(opportunity.efficientBatchTt).toBeCloseTo(0.5 / (0.1 * 29), 4);
	});

	it('preserves a modest-premium month market as thin when an efficient batch fits', () => {
		const opportunity = marketOpportunity(obs('X', 110, 'month', 360), 100.6);
		expect(opportunity.kind).toBe('thin');
		expect(opportunity.usesNanocube).toBe(false);
		expect(opportunity.efficientBatchTt).toBeCloseTo(50, 4);
		expect(opportunity.efficientBatchMarketShare).toBeCloseTo(50 / 360, 4);
		expect(opportunity.efficientBatchMarketWeeks).toBeCloseTo(50 / (360 / 4.345), 4);
	});

	it('uses the recycling floor for unsupported or economically inferior direct markets', () => {
		const unsupported = marketOpportunity(obs('X', 101, 'year', 1), 100.6);
		expect(unsupported.kind).toBe('recycle');
		expect(unsupported.appliedMarkupPct).toBe(100.6);

		const inferior = marketOpportunity(obs('X', 100.2, 'week', 1_000_000), 100.6);
		expect(inferior.kind).toBe('recycle');
		expect(inferior.appliedMarkupPct).toBe(100.6);

		const uncovered = marketOpportunity(undefined, 100.84);
		expect(uncovered.kind).toBe('recycle');
		expect(uncovered.appliedMarkupPct).toBe(100.84);
	});

	it('maps opportunity evidence onto the established confidence chrome', () => {
		const broad = marketOpportunity(obs('Broad', 110, 'week', 10_000), 100.6);
		const niche = marketOpportunity(obs('Niche', 3000, 'month', 20), 100.6);
		const thin = marketOpportunity(obs('Thin', 110, 'month', 360), 100.6);

		expect(opportunityTier(broad)).toBe('liquid');
		expect(opportunityTier(niche)).toBe('middling');
		expect(opportunityTier(thin)).toBe('illiquid');
		expect(effectiveMarkup(thin, 100.6, 'liquidMiddling')).toEqual({
			markupPct: 100.6,
			floored: true,
		});
		expect(effectiveMarkup(thin, 100.6, 'all')).toEqual({
			markupPct: 110,
			floored: false,
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
	it('applies holding-independent opportunity to every sub-activity', async () => {
		mocked.getAnalyticsHarvest.mockResolvedValue(harvest());
		mocked.getMarketHarvestMarkups.mockResolvedValue(market());
		const model = createTreeCuttingModel();

		await model.loadData();

		const long = sectionOf(model, PH1).items[0];
		expect(long.opportunity.kind).toBe('broad');
		expect(long.opportunity.usesNanocube).toBe(false);
		expect(long.opportunity.appliedMarkupPct).toBe(353.69);
		expect(long.opportunity.salesPed).toBe(320.34);
		expect(long.opportunity.weeklySalesPed).toBe(320.34);

		const wood = sectionOf(model, PH4).items[0];
		expect(wood.opportunity.kind).toBe('thin');
		expect(wood.opportunity.usesNanocube).toBe(false);
		expect(wood.opportunity.appliedMarkupPct).toBe(110.01);
		// Un-normalised fallback volume and zero weekly sales remain visible
		// evidence, but neither consults the player's holding.
		expect(wood.opportunity.salesPed).toBe(363.61);
		expect(wood.opportunity.weeklySalesPed).toBe(0);
	});

	it('combines every tool into the overall aggregate', async () => {
		mocked.getAnalyticsHarvest.mockResolvedValue(harvest());
		mocked.getMarketHarvestMarkups.mockResolvedValue(market());
		const model = createTreeCuttingModel();
		await model.loadData();

		const overall = required(model.overall, 'overall stats');
		// Cycled and returns sum across both tools; rate is volume-weighted.
		expect(overall.cycled).toBeCloseTo(91.24 + 111.13, 4);
		expect(overall.returns).toBeCloseTo(34.26 + 87.38, 4);
		expect(overall.lootRate).toBeCloseTo((34.26 + 87.38) / (91.24 + 111.13), 4);
		// Current market sums the per-section holding-independent figures.
		const firstMarket = required(model.sections[0].muProjectedReturns, 'first market return');
		const secondMarket = required(model.sections[1].muProjectedReturns, 'second market return');
		expect(overall.muProjectedReturns).toBeCloseTo(firstMarket + secondMarket, 4);
		expect(overall.realisedReturns).toBeCloseTo(overall.returns, 4);
		expect(overall.realisedRate).toBeCloseTo(overall.lootRate, 4);
	});

	it('drops overall market figures when the market feed is unavailable', async () => {
		mocked.getAnalyticsHarvest.mockResolvedValue(harvest());
		mocked.getMarketHarvestMarkups.mockResolvedValue(null as unknown as MarketHarvestData);
		const model = createTreeCuttingModel();
		await model.loadData();
		const overall = required(model.overall, 'overall stats');
		expect(overall.muProjectedReturns).toBeNull();
		expect(overall.muRate).toBeNull();
		expect(overall.returns).toBeCloseTo(34.26 + 87.38, 4);
	});

	it('does not change activity opportunity when current holdings change', async () => {
		mocked.getAnalyticsHarvest.mockResolvedValue(harvest());
		mocked.getMarketHarvestMarkups.mockResolvedValue(market());
		const model = createTreeCuttingModel();
		await model.loadData();

		const before = sectionOf(model, PH1).muProjectedReturns;
		await model.setHeld('Long Moonleaf Board', 3);
		expect(sectionOf(model, PH1).muProjectedReturns).toBe(before);
		expect(sectionOf(model, PH1).items[0].opportunity.kind).toBe('broad');
	});

	it('lets the confidence toggle choose which supported MU tiers feed the aggregate', async () => {
		mocked.getAnalyticsHarvest.mockResolvedValue(harvest());
		mocked.getMarketHarvestMarkups.mockResolvedValue(market());
		const model = createTreeCuttingModel();
		await model.loadData();

		const woodAtDefault = sectionOf(model, PH4);
		expect(model.confidenceMode).toBe('liquidMiddling');
		expect(woodAtDefault.items[0].tier).toBe('illiquid');
		expect(woodAtDefault.items[0].floored).toBe(true);
		expect(woodAtDefault.items[0].effectiveMarkupPct).toBe(100.84);

		model.confidenceMode = 'all';
		const woodAtAll = sectionOf(model, PH4);
		expect(woodAtAll.items[0].floored).toBe(false);
		expect(woodAtAll.items[0].effectiveMarkupPct).toBe(110.01);
		expect(woodAtAll.muProjectedReturns).toBeGreaterThan(
			required(woodAtDefault.muProjectedReturns, 'default MU return'),
		);
	});

	it('uses the nanocube fallback constant for an uncovered item when the feed lacks it', async () => {
		mocked.getAnalyticsHarvest.mockResolvedValue(harvest());
		mocked.getMarketHarvestMarkups.mockResolvedValue({
			nanocubeMarkupPct: null,
			items: [],
		});
		const model = createTreeCuttingModel();
		await model.loadData();
		expect(model.sections[0].items[0].opportunity.appliedMarkupPct).toBe(NANOCUBE_FALLBACK_MARKUP);
		expect(model.sections[0].items[0].opportunity.kind).toBe('recycle');
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
		const long = stockOf(model, 'Long Moonleaf Board');
		expect(long.lootedQty).toBe(571);
		expect(long.removedQty).toBe(0);
		expect(long.heldQty).toBe(571);
		expect(long.heldTt).toBeCloseTo(34.26, 4);
	});

	it('reflects a removed overlay loaded from the backend', async () => {
		mocked.getAnalyticsHarvest.mockResolvedValue(harvest());
		mocked.getHarvestStock.mockResolvedValue([{ itemName: 'Long Moonleaf Board', removedQty: 71 }]);
		const model = createTreeCuttingModel();
		await model.loadData();

		const long = stockOf(model, 'Long Moonleaf Board');
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
		expect(stockOf(model, 'Long Moonleaf Board').heldQty).toBe(500);
	});

	it('keeps market opportunity stable while selling down current stock', async () => {
		mocked.getAnalyticsHarvest.mockResolvedValue(harvest());
		mocked.getMarketHarvestMarkups.mockResolvedValue(market());
		const model = createTreeCuttingModel();
		await model.loadData();

		expect(sectionOf(model, PH1).items[0].opportunity.kind).toBe('broad');

		// Selling changes current stock, not what the observed market says
		// about repeating the source activity.
		await model.setHeld('Long Moonleaf Board', 3);
		expect(stockOf(model, 'Long Moonleaf Board').heldQty).toBe(3);
		expect(sectionOf(model, PH1).items[0].opportunity.kind).toBe('broad');
		expect(sectionOf(model, PH1).items[0].opportunity.appliedMarkupPct).toBe(353.69);
	});

	it('joins each stock row to its resolved markup and horizon breakdown', async () => {
		mocked.getAnalyticsHarvest.mockResolvedValue(harvest());
		mocked.getMarketHarvestMarkups.mockResolvedValue(market());
		const model = createTreeCuttingModel();
		await model.loadData();

		const long = stockOf(model, 'Long Moonleaf Board');
		expect(long.opportunity?.ownMarkupPct).toBe(353.69);
		expect(long.opportunity?.horizon).toBe('week');
		expect(long.opportunity?.kind).toBe('broad');
		// Ordered day, week, month, year, from the synthesised breakdown.
		expect(long.readings.map((r) => r.horizon)).toEqual(['day', 'week', 'month', 'year']);
		expect(long.readings.find((r) => r.horizon === 'week')?.markupPct).toBe(353.69);

		const wood = stockOf(model, 'Wood Shavings');
		expect(wood.opportunity?.ownMarkupPct).toBe(110.01);
		expect(wood.opportunity?.horizon).toBe('month');
		expect(wood.opportunity?.kind).toBe('thin');
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
		expect(stockOf(model, 'Long Moonleaf Board').removedQty).toBe(0);
	});
});
