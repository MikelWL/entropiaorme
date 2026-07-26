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
	harvestTierLabel,
	marketOpportunity,
	NANOCUBE_FALLBACK_MARKUP,
	opportunityTier,
	treeCuttingActivityName,
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
// section by its durable yield tier rather than a positional index.
const HUGE = 'huge';
const LONG = 'long';

function required<T>(value: T | null | undefined, label: string): T {
	if (value == null) throw new Error(`Expected ${label}`);
	return value;
}

const sectionOf = (model: ReturnType<typeof createTreeCuttingModel>, tier: string) =>
	required(
		model.sections.find((section) => section.yieldTier === tier),
		`section ${tier}`,
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

// Exercises broad and thin market opportunities together. Tier order here is
// immaterial because the model sorts sections itself; the end-to-end fixture,
// which the UI renders in payload order, mirrors the emitted rank order instead.
function harvest(): AnalyticsHarvest {
	return {
		tierComparisons: [
			{
				yieldTier: 'huge',
				swings: 4562,
				cycled: 91.24,
				returns: 34.26,
				lootRate: 0.3755,
				lootItems: [loot('Long Moonleaf Board', 571, 34.26)],
				toolComparisons: [
					{
						toolName: 'Terratech PH-3',
						swings: 4,
						cycled: 0.4,
						returns: 0.3,
						lootRate: 0.75,
						lootItems: [loot('Long Moonleaf Board', 5, 0.3)],
					},
					{
						toolName: 'Terratech PH-4 (L)',
						swings: 4558,
						cycled: 90.84,
						returns: 33.96,
						lootRate: 0.3738,
						lootItems: [loot('Long Moonleaf Board', 566, 33.96)],
					},
				],
			},
			{
				yieldTier: 'long',
				swings: 127,
				cycled: 111.13,
				returns: 87.38,
				lootRate: 0.7863,
				lootItems: [loot('Wood Shavings', 87431, 87.38)],
				toolComparisons: [
					{
						toolName: 'Terratech PH-4 (L)',
						swings: 127,
						cycled: 111.13,
						returns: 87.38,
						lootRate: 0.7863,
						lootItems: [loot('Wood Shavings', 87431, 87.38)],
					},
				],
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

describe('harvestTierLabel', () => {
	it('maps the durable vocabulary to its UI labels', () => {
		expect(harvestTierLabel('short')).toBe('Short Boards');
		expect(harvestTierLabel('long')).toBe('Boards');
		expect(harvestTierLabel('huge')).toBe('Long Boards');
		expect(harvestTierLabel('unknown')).toBe('Unclassified');
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
		const weeklyThin = marketOpportunity(obs('Weekly thin', 105.29, 'week', 603.08), 100.84);
		const fallbackThin = marketOpportunity(obs('Fallback thin', 110, 'month', 360), 100.6);
		const unsupportedWeekly = marketOpportunity(obs('Unsupported weekly', 105, 'week', 100), 100.6);

		expect(opportunityTier(broad)).toBe('liquid');
		expect(opportunityTier(niche)).toBe('middling');
		expect(weeklyThin.kind).toBe('thin');
		expect(opportunityTier(weeklyThin)).toBe('middling');
		expect(effectiveMarkup(weeklyThin, 100.84, 'liquidMiddling')).toEqual({
			markupPct: 105.29,
			floored: false,
		});
		expect(opportunityTier(fallbackThin)).toBe('illiquid');
		expect(effectiveMarkup(fallbackThin, 100.6, 'liquidMiddling')).toEqual({
			markupPct: 100.6,
			floored: true,
		});
		expect(effectiveMarkup(fallbackThin, 100.6, 'all')).toEqual({
			markupPct: 110,
			floored: false,
		});
		expect(unsupportedWeekly.kind).toBe('recycle');
		expect(opportunityTier(unsupportedWeekly)).toBe('illiquid');
	});
});

describe('loadData', () => {
	it('maps the selected analytics range to the backend period', () => {
		const model = createTreeCuttingModel();
		expect(model.period).toBe('all');
		model.activeRange = '30d';
		expect(model.period).toBe('30d');
		model.activeRange = '90d';
		expect(model.period).toBe('90d');
	});

	it('surfaces a load failure', async () => {
		mocked.getAnalyticsHarvest.mockRejectedValue(new Error('backend unreachable'));
		const model = createTreeCuttingModel();
		await model.loadData();
		expect(model.error).toBe('backend unreachable');
		expect(model.data).toBeNull();
	});

	it('scopes activity evidence while keeping current stock all-time', async () => {
		const allTime = harvest();
		const recent: AnalyticsHarvest = { tierComparisons: [allTime.tierComparisons[0]] };
		mocked.getAnalyticsHarvest.mockImplementation(async (period = 'all') =>
			period === '30d' ? recent : allTime,
		);
		mocked.getMarketHarvestMarkups.mockResolvedValue(market());
		const model = createTreeCuttingModel();

		model.activeRange = '30d';
		await model.loadData(model.period);

		expect(mocked.getAnalyticsHarvest).toHaveBeenCalledWith('30d');
		expect(mocked.getAnalyticsHarvest).toHaveBeenCalledWith('all');
		expect(model.sections.map((section) => section.yieldTier)).toEqual([HUGE]);
		expect(model.stock.map((item) => item.itemName)).toEqual([
			'Wood Shavings',
			'Long Moonleaf Board',
		]);
	});
});

describe('sections', () => {
	it('applies holding-independent opportunity to every sub-activity', async () => {
		mocked.getAnalyticsHarvest.mockResolvedValue(harvest());
		mocked.getMarketHarvestMarkups.mockResolvedValue(market());
		const model = createTreeCuttingModel();

		await model.loadData();

		const long = sectionOf(model, HUGE).items[0];
		expect(long.opportunity.kind).toBe('broad');
		expect(long.opportunity.usesNanocube).toBe(false);
		expect(long.opportunity.appliedMarkupPct).toBe(353.69);
		expect(long.opportunity.salesPed).toBe(320.34);
		expect(long.opportunity.weeklySalesPed).toBe(320.34);

		const wood = sectionOf(model, LONG).items[0];
		expect(wood.opportunity.kind).toBe('thin');
		expect(wood.opportunity.usesNanocube).toBe(false);
		expect(wood.opportunity.appliedMarkupPct).toBe(110.01);
		// Un-normalised fallback volume and zero weekly sales remain visible
		// evidence, but neither consults the player's holding.
		expect(wood.opportunity.salesPed).toBe(363.61);
		expect(wood.opportunity.weeklySalesPed).toBe(0);
	});

	it('keeps PH-3 and PH-4 as separate strategies inside the Long Boards activity', async () => {
		mocked.getAnalyticsHarvest.mockResolvedValue(harvest());
		mocked.getMarketHarvestMarkups.mockResolvedValue(market());
		const model = createTreeCuttingModel();
		await model.loadData();

		const huge = sectionOf(model, HUGE);
		expect(huge.tools.map((tool) => tool.toolName)).toEqual([
			'Terratech PH-3',
			'Terratech PH-4 (L)',
		]);
		expect(huge.tools[0].cycled).toBe(0.4);
		expect(huge.tools[0].muRate).not.toBeNull();
		expect(huge.tools[1].cycled).toBe(90.84);
	});

	it('surfaces genuine tier and tool attribution gaps explicitly', async () => {
		const data = harvest();
		data.tierComparisons.push({
			yieldTier: 'unknown',
			swings: 1,
			cycled: 0.1,
			returns: 0,
			lootRate: 0,
			lootItems: [],
			toolComparisons: [
				{
					toolName: null,
					swings: 1,
					cycled: 0.1,
					returns: 0,
					lootRate: 0,
					lootItems: [],
				},
			],
		});
		mocked.getAnalyticsHarvest.mockResolvedValue(data);
		const model = createTreeCuttingModel();
		await model.loadData();

		const unknown = sectionOf(model, 'unknown');
		expect(harvestTierLabel(unknown.yieldTier)).toBe('Unclassified');
		expect(treeCuttingActivityName(unknown)).toBe('Unclassified');
		expect(treeCuttingActivityName(sectionOf(model, HUGE))).toBe('Long Boards');
		expect(unknown.tools[0].toolName).toBe('Unknown tool');
	});

	it('keeps Unclassified last and out of the default selection', async () => {
		const data = harvest();
		data.tierComparisons.push({
			yieldTier: 'unknown',
			swings: 400,
			cycled: 400,
			returns: 0,
			lootRate: 0,
			lootItems: [],
			toolComparisons: [],
		});
		mocked.getAnalyticsHarvest.mockResolvedValue(data);
		const model = createTreeCuttingModel();
		await model.loadData();

		expect(model.sections.map((section) => section.yieldTier)).toEqual([LONG, HUGE, 'unknown']);
		expect(model.selectedSection?.yieldTier).toBe(LONG);
	});

	it('combines every yield tier into the overall aggregate', async () => {
		mocked.getAnalyticsHarvest.mockResolvedValue(harvest());
		mocked.getMarketHarvestMarkups.mockResolvedValue(market());
		const model = createTreeCuttingModel();
		await model.loadData();

		const overall = required(model.overall, 'overall stats');
		// Cycled and returns sum across both tiers; rate is volume-weighted.
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

		const before = sectionOf(model, HUGE).muProjectedReturns;
		await model.setHeld('Long Moonleaf Board', 3);
		expect(sectionOf(model, HUGE).muProjectedReturns).toBe(before);
		expect(sectionOf(model, HUGE).items[0].opportunity.kind).toBe('broad');
	});

	it('lets the confidence toggle choose which supported MU tiers feed the aggregate', async () => {
		mocked.getAnalyticsHarvest.mockResolvedValue(harvest());
		mocked.getMarketHarvestMarkups.mockResolvedValue(market());
		const model = createTreeCuttingModel();
		await model.loadData();

		const woodAtDefault = sectionOf(model, LONG);
		const woodStockAtDefault = stockOf(model, 'Wood Shavings');
		expect(model.confidenceMode).toBe('liquidMiddling');
		expect(woodAtDefault.items[0].tier).toBe('illiquid');
		expect(woodAtDefault.items[0].floored).toBe(true);
		expect(woodAtDefault.items[0].effectiveMarkupPct).toBe(100.84);
		expect(woodStockAtDefault.floored).toBe(true);
		expect(woodStockAtDefault.effectiveMarkupPct).toBe(100.84);

		model.confidenceMode = 'all';
		const woodAtAll = sectionOf(model, LONG);
		const woodStockAtAll = stockOf(model, 'Wood Shavings');
		expect(woodAtAll.items[0].floored).toBe(false);
		expect(woodAtAll.items[0].effectiveMarkupPct).toBe(110.01);
		expect(woodStockAtAll.floored).toBe(false);
		expect(woodStockAtAll.effectiveMarkupPct).toBe(110.01);
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
		const long = sectionOf(model, HUGE);
		expect(long.muProjectedReturns).toBeNull();
		expect(long.muRate).toBeNull();
		expect(long.returns).toBe(34.26);
	});

	it('orders sections by cycled volume and opens the busiest by default', async () => {
		mocked.getAnalyticsHarvest.mockResolvedValue(harvest());
		const model = createTreeCuttingModel();
		await model.loadData();

		// Long cycled 111.13 outranks Huge's 91.24, so it leads the list and
		// opens as the default selection.
		expect(model.sections.map((s) => s.yieldTier)).toEqual([LONG, HUGE]);
		expect(model.selectedSection?.yieldTier).toBe(LONG);

		// Selecting another sub-activity swaps the open detail.
		model.selectSection('huge');
		expect(model.selectedSection?.yieldTier).toBe(HUGE);
	});

	it('sorts the activity comparison by either economic rate without changing selection', async () => {
		mocked.getAnalyticsHarvest.mockResolvedValue(harvest());
		mocked.getMarketHarvestMarkups.mockResolvedValue(market());
		const model = createTreeCuttingModel();
		await model.loadData();

		expect(model.activityTable.sortKey).toBe('cycled');
		expect(model.activityTable.sortDir).toBe('desc');
		expect(model.activityTable.filtered.map((s) => s.yieldTier)).toEqual([LONG, HUGE]);

		model.selectSection('long');
		model.activityTable.setSort('realisedRate');
		expect(model.activityTable.sortDir).toBe('desc');
		// Realised is the TT loot rate today, and Long Boards both cycles more and
		// returns a better rate here, so this order matches the cycled order. The
		// MU Rate case below is the one proving a rate sort can reorder the table,
		// because market evidence is independent of the TT rate. Making realised
		// reorder instead would mean new totals, not new rates: these totals
		// cannot produce it while each rate stays derived from its own.
		expect(model.activityTable.filtered.map((s) => s.yieldTier)).toEqual([LONG, HUGE]);
		expect(model.selectedSection?.yieldTier).toBe(LONG);

		model.activityTable.setSort('muRate');
		expect(model.activityTable.sortDir).toBe('desc');
		expect(model.activityTable.filtered.map((s) => s.yieldTier)).toEqual([HUGE, LONG]);
		expect(model.selectedSection?.yieldTier).toBe(LONG);
	});

	it('reactively reorders an active MU Rate sort when confidence changes', async () => {
		mocked.getAnalyticsHarvest.mockResolvedValue(harvest());
		const changingMarket = market();
		changingMarket.items = [
			obs('Long Moonleaf Board', 300, 'week', 320.34, 320.34),
			obs('Wood Shavings', 150, 'month', 363.61, 0),
		];
		mocked.getMarketHarvestMarkups.mockResolvedValue(changingMarket);
		const model = createTreeCuttingModel();
		await model.loadData();

		model.activityTable.setSort('muRate');
		expect(model.activityTable.filtered.map((s) => s.yieldTier)).toEqual([HUGE, LONG]);

		model.confidenceMode = 'all';
		expect(model.activityTable.filtered.map((s) => s.yieldTier)).toEqual([LONG, HUGE]);
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

		expect(sectionOf(model, HUGE).items[0].opportunity.kind).toBe('broad');

		// Selling changes current stock, not what the observed market says
		// about repeating the source activity.
		await model.setHeld('Long Moonleaf Board', 3);
		expect(stockOf(model, 'Long Moonleaf Board').heldQty).toBe(3);
		expect(sectionOf(model, HUGE).items[0].opportunity.kind).toBe('broad');
		expect(sectionOf(model, HUGE).items[0].opportunity.appliedMarkupPct).toBe(353.69);
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
