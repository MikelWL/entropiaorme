import { beforeEach, describe, expect, it, vi } from 'vitest';
import type {
	AnalyticsHuntingActivity,
	HuntingDefinitionComparison,
	HuntingSignature,
	HuntingSpeciesComparison,
	MarketHarvestItem,
} from '$lib/api/commands.gen';
import { createHuntingModel, instanceTrend, signatureEconomics } from './huntingModel.svelte';

vi.mock('$lib/api', () => ({
	getAnalyticsHuntingActivity: vi.fn(),
	getMarketHuntMarkups: vi.fn(),
	getActivityStock: vi.fn(),
	getAuctionListings: vi.fn(),
	getHuntingRealisedMarkup: vi.fn(),
	createAuctionListing: vi.fn(),
	confirmAuctionListing: vi.fn(),
	expireAuctionListing: vi.fn(),
	convertStock: vi.fn(),
	getActivityHistory: vi.fn(),
	revertAuctionSale: vi.fn(),
	undoAuctionListing: vi.fn(),
	undoStockConversion: vi.fn(),
}));

import * as api from '$lib/api';

const mocked = vi.mocked(api);

function required<T>(value: T | null | undefined, label: string): T {
	if (value == null) throw new Error(`Expected ${label}`);
	return value;
}

function signature(over: Partial<HuntingSignature> = {}): HuntingSignature {
	return {
		kind: 'quest',
		label: 'Daily Hunting 1: Weak Mortirex',
		runs: 4,
		kills: 120,
		durationHours: 1.6,
		cycled: 400,
		returns: 360,
		pes: 30,
		rewardPed: 4,
		rewardIsSkill: false,
		expectedRewardMarkupPercent: 150,
		variants: [],
		...over,
	};
}

function definition(over: Partial<HuntingDefinitionComparison> = {}): HuntingDefinitionComparison {
	return {
		definitionId: 1,
		name: 'ARIS Dailies',
		isArchived: false,
		instances: 5,
		kills: 320,
		durationHours: 4.5,
		cycled: 1200,
		returns: 1080,
		lootRate: 0.9,
		pes: 98.4,
		pesPer100Ped: 8.2,
		activities: [signature()],
		mobs: [{ mobSpecies: 'Atrox', kills: 250, lootTt: 830 }],
		instanceRows: [],
		...over,
	};
}

function species(over: Partial<HuntingSpeciesComparison> = {}): HuntingSpeciesComparison {
	return {
		mobSpecies: 'Atrox',
		kills: 250,
		cycled: 900,
		returns: 810,
		lootRate: 0.9,
		pes: 80,
		pesPer100Ped: 8.89,
		pesSessions: 3,
		maturities: [{ maturity: 'Young', kills: 250, cycled: 900, returns: 810, lootRate: 0.9 }],
		lootItems: [{ itemName: 'Animal Muscle Oil', quantity: 400, valuePed: 120 }],
		...over,
	};
}

function activity(over: Partial<AnalyticsHuntingActivity> = {}): AnalyticsHuntingActivity {
	return {
		overall: {
			sessions: 6,
			kills: 400,
			durationHours: 6,
			cycled: 1500,
			returns: 1350,
			lootRate: 0.9,
			pes: 120,
			pesPer100Ped: 8,
		},
		definitions: [definition(), definition({ definitionId: null, name: 'Unassigned' })],
		species: [
			species(),
			species({ mobSpecies: '', pes: null, pesPer100Ped: null, pesSessions: 0 }),
		],
		...over,
	};
}

function obs(
	name: string,
	markupPct: number | null,
	horizon: string | null,
	salesPed: number | null,
): MarketHarvestItem {
	const readings = ['day', 'week', 'month', 'year'].map((h) => ({
		horizon: h,
		markupPct: h === horizon ? markupPct : null,
		salesPed: h === horizon ? (salesPed ?? 0) : 0,
	}));
	return { itemName: name, markupPct, horizon, salesPed, readings };
}

beforeEach(() => {
	vi.clearAllMocks();
	mocked.getAnalyticsHuntingActivity.mockResolvedValue(activity());
	mocked.getMarketHuntMarkups.mockResolvedValue({
		nanocubeMarkupPct: 100.6,
		items: [obs('Animal Muscle Oil', 130, 'week', 5000)],
	});
	mocked.getActivityStock.mockResolvedValue([
		{ itemName: 'Animal Muscle Oil', quantity: 400, ttValue: 120, listedQuantity: 0 },
	]);
	mocked.getAuctionListings.mockResolvedValue([]);
	mocked.getHuntingRealisedMarkup.mockResolvedValue([{ mobSpecies: 'Atrox', netMarkup: 12 }]);
	mocked.getActivityHistory.mockResolvedValue([]);
	mocked.createAuctionListing.mockResolvedValue({} as never);
	mocked.convertStock.mockResolvedValue(undefined);
});

describe('createHuntingModel', () => {
	it('scopes every stock and lifecycle read to the hunting activity', async () => {
		const model = createHuntingModel();
		await model.loadData();

		expect(mocked.getActivityStock).toHaveBeenCalledWith('hunting');
		expect(mocked.getAuctionListings).toHaveBeenCalledWith('hunting');
		// History reads only when the surface is opened.
		expect(mocked.getActivityHistory).not.toHaveBeenCalled();
		await model.loadHistory();
		expect(mocked.getActivityHistory).toHaveBeenCalledWith('hunting');
	});

	it('keys sessions by definition and pins the unassigned bucket', async () => {
		const model = createHuntingModel();
		await model.loadData();

		const keys = model.sessionSections.map((section) => section.key);
		expect(keys).toContain('definition:1');
		expect(keys).toContain('unassigned');
		const unassigned = required(
			model.sessionSections.find((section) => section.isUnassigned),
			'unassigned bucket',
		);
		expect(unassigned.name).toBe('Unassigned');
	});

	it('merges market opportunity and realised markup into the target rows', async () => {
		const model = createHuntingModel();
		await model.loadData();

		const atrox = required(
			model.targetSections.find((section) => section.mobSpecies === 'Atrox'),
			'Atrox target',
		);
		// 120 TT of oil at 130% projects 156; realised markup adds 12 on top
		// of the 810 TT returns.
		expect(required(atrox.muProjectedReturns, 'MU projection')).toBeCloseTo(156, 5);
		expect(atrox.realisedMarkup).toBeCloseTo(12, 5);
		expect(atrox.realisedReturns).toBeCloseTo(822, 5);
		expect(atrox.realisedRate).toBeCloseTo(822 / 900, 5);

		const unclassified = required(
			model.targetSections.find((section) => section.isUnclassified),
			'unclassified bucket',
		);
		expect(unclassified.label).toBe('Unclassified');
	});

	it('reconciles the Overall market figures with the target rows', async () => {
		const model = createHuntingModel();
		await model.loadData();

		const overall = required(model.overall, 'overall');
		const projected = model.targetSections.reduce(
			(sum, section) => sum + (section.muProjectedReturns ?? 0),
			0,
		);
		expect(required(overall.muProjectedReturns, 'MU aggregate')).toBeCloseTo(projected, 5);
		expect(overall.realisedMarkup).toBeCloseTo(12, 5);
		expect(overall.realisedRate).toBeCloseTo((1350 + 12) / 1500, 5);
	});

	it('degrades a stale selection to the first row instead of an empty pane', async () => {
		const model = createHuntingModel();
		await model.loadData();

		model.selectTarget('species:Berycled');
		expect(required(model.selectedTarget, 'fallback target').mobSpecies).toBe('Atrox');
		model.selectSession('definition:99');
		expect(required(model.selectedSession, 'fallback session').definitionId).toBe(1);
	});

	it('stamps the hunting profession on listings and conversions', async () => {
		const model = createHuntingModel();
		await model.loadData();

		await model.listStock({
			itemName: 'Animal Muscle Oil',
			quantity: 10,
			startingBid: 1,
			buyout: null,
			listingFee: 0.5,
			listedAt: null,
		});
		expect(mocked.createAuctionListing).toHaveBeenCalledWith(
			expect.objectContaining({ profession: 'hunting', itemName: 'Animal Muscle Oil' }),
		);

		await model.recycleStock('Animal Muscle Oil', 5);
		expect(mocked.convertStock).toHaveBeenCalledWith(
			expect.objectContaining({ profession: 'hunting', targetItem: 'Nanocube' }),
		);
	});

	it('surfaces a spine failure while letting the market feed degrade quietly', async () => {
		mocked.getMarketHuntMarkups.mockRejectedValue(new Error('offline'));
		const model = createHuntingModel();
		await model.loadData();
		expect(model.error).toBeNull();
		const atrox = required(
			model.targetSections.find((section) => section.mobSpecies === 'Atrox'),
			'Atrox target',
		);
		expect(atrox.muProjectedReturns).toBeNull();

		mocked.getAnalyticsHuntingActivity.mockRejectedValue(new Error('backend unreachable'));
		const broken = createHuntingModel();
		await broken.loadData();
		expect(broken.error).toContain('backend unreachable');
	});
});

describe('signatureEconomics', () => {
	it('reads the shortfall, reward, and voucher scenario per run', () => {
		const economics = signatureEconomics(signature());
		// 400 cycled less 360 returned across 4 runs: 10 PED short per run.
		expect(required(economics.shortfallPerRun, 'shortfall')).toBeCloseTo(10, 5);
		expect(economics.rewardPed).toBe(4);
		expect(required(economics.netAfterRewardPerRun, 'net after reward')).toBeCloseTo(-6, 5);
		// 4 PED at 150% is 6 PED; still 4 short of the 10 PED shortfall.
		expect(required(economics.voucherScenarioPerRun, 'voucher scenario')).toBeCloseTo(-4, 5);
	});

	it('never converts a skill reward into a liquid figure', () => {
		const economics = signatureEconomics(signature({ rewardIsSkill: true }));
		expect(economics.rewardPed).toBeNull();
		expect(economics.netAfterRewardPerRun).toBeNull();
		expect(economics.voucherScenarioPerRun).toBeNull();
	});

	it('reports no per-run readout without recorded runs', () => {
		const economics = signatureEconomics(signature({ runs: 0 }));
		expect(economics.shortfallPerRun).toBeNull();
		expect(economics.netAfterRewardPerRun).toBeNull();
	});
});

describe('instanceTrend', () => {
	const row = (cycled: number, returns: number) => ({ cycled, returns });

	it('says nothing below eight instances: a thin sample is not a verdict', () => {
		expect(instanceTrend([row(100, 95), row(100, 95), row(100, 80), row(100, 80)])).toBeNull();
	});

	it('compares the newer half against the older half', () => {
		// Newest first: four recent runs at 95%, four older at 80%.
		const improving = [...Array(4).fill(row(100, 95)), ...Array(4).fill(row(100, 80))];
		const declining = [...Array(4).fill(row(100, 80)), ...Array(4).fill(row(100, 95))];
		const steady = Array(8).fill(row(100, 90));
		expect(instanceTrend(improving)).toBe('improving');
		expect(instanceTrend(declining)).toBe('declining');
		expect(instanceTrend(steady)).toBe('stable');
	});
});
