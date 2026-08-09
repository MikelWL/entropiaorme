import { beforeEach, describe, expect, it, vi } from 'vitest';
import type {
	AnalyticsHuntingActivity,
	HuntingActivityComparison,
	HuntingDefinitionComparison,
	HuntingSpeciesComparison,
	MarketHarvestItem,
} from '$lib/api/commands.gen';
import { createHuntingModel } from './huntingModel.svelte';

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

function definition(over: Partial<HuntingDefinitionComparison> = {}): HuntingDefinitionComparison {
	return {
		definitionId: 1,
		name: 'ARIS Dailies',
		isArchived: false,
		cycled: 1200,
		returns: 1080,
		lootRate: 0.9,
		lootItems: [{ itemName: 'Animal Muscle Oil', quantity: 400, valuePed: 120 }],
		activities: [],
		...over,
	};
}

function species(over: Partial<HuntingSpeciesComparison> = {}): HuntingSpeciesComparison {
	return {
		mobSpecies: 'Atrox',
		cycled: 900,
		returns: 810,
		lootRate: 0.9,
		lootItems: [{ itemName: 'Animal Muscle Oil', quantity: 400, valuePed: 120 }],
		...over,
	};
}

function sessionActivity(over: Partial<HuntingActivityComparison> = {}): HuntingActivityComparison {
	return {
		kind: 'quest',
		label: 'Daily Hunting 1',
		cycled: 100,
		returns: 90,
		lootRate: 0.9,
		confirmedRewardPed: 15,
		rewardedReturns: 105,
		rewardedRate: 1.05,
		rewardStatus: 'fixed_liquid',
		lootItems: [{ itemName: 'Animal Muscle Oil', quantity: 40, valuePed: 12 }],
		variants: [],
		...over,
	};
}

function activity(over: Partial<AnalyticsHuntingActivity> = {}): AnalyticsHuntingActivity {
	return {
		overall: { cycled: 1500, returns: 1350, lootRate: 0.9 },
		definitions: [
			definition(),
			definition({ definitionId: null, name: 'Unassigned', cycled: 300, returns: 270 }),
		],
		species: [species(), species({ mobSpecies: '', cycled: 600, returns: 540, lootItems: [] })],
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
	mocked.getHuntingRealisedMarkup.mockResolvedValue({
		species: [{ mobSpecies: 'Atrox', netMarkup: 12 }],
		definitions: [{ definitionId: 1, netMarkup: 18 }],
	});
	mocked.getActivityHistory.mockResolvedValue([]);
	mocked.createAuctionListing.mockResolvedValue({} as never);
	mocked.convertStock.mockResolvedValue(undefined);
});

describe('createHuntingModel', () => {
	it('scopes every stock and lifecycle read to hunting', async () => {
		const model = createHuntingModel();
		await model.loadData();

		expect(mocked.getActivityStock).toHaveBeenCalledWith('hunting');
		expect(mocked.getAuctionListings).toHaveBeenCalledWith('hunting');
		expect(mocked.getActivityHistory).not.toHaveBeenCalled();
		await model.loadHistory();
		expect(mocked.getActivityHistory).toHaveBeenCalledWith('hunting');
	});

	it('projects the Tree Cutting economics for each session definition', async () => {
		const model = createHuntingModel();
		await model.loadData();

		const session = required(
			model.sessionSections.find((section) => section.definitionId === 1),
			'ARIS session',
		);
		expect(session.key).toBe('definition:1');
		expect(required(session.muProjectedReturns, 'MU projection')).toBeCloseTo(156, 5);
		expect(session.realisedMarkup).toBe(18);
		expect(session.realisedReturns).toBe(1098);
		expect(session.realisedRate).toBeCloseTo(1098 / 1200, 5);
		expect(session.items.map((item) => item.name)).toEqual(['Animal Muscle Oil']);

		const unassigned = required(
			model.sessionSections.find((section) => section.isUnassigned),
			'unassigned bucket',
		);
		expect(unassigned.key).toBe('unassigned');
		expect(unassigned.realisedMarkup).toBe(0);
	});

	it('adds confirmed rewards once to activity, session, and Overall realised outcomes', async () => {
		const variant = sessionActivity();
		mocked.getAnalyticsHuntingActivity.mockResolvedValue(
			activity({
				definitions: [
					definition({
						activities: [sessionActivity({ kind: 'quest_family', variants: [variant] })],
					}),
				],
			}),
		);
		const model = createHuntingModel();
		await model.loadData();
		model.selectSession('definition:1');

		const row = required(model.selectedSession?.activities[0], 'rewarded activity');
		expect(row.rewardedRate).toBeCloseTo(1.05, 5);
		expect(required(row.muProjectedReturns, 'activity MU projection')).toBeCloseTo(15.6, 5);
		const session = required(model.selectedSession, 'rewarded session');
		expect(session.confirmedRewardPed).toBe(15);
		expect(session.realisedReturns).toBe(1113);
		expect(session.realisedRate).toBeCloseTo(1113 / 1200, 5);
		const overall = required(model.overall, 'rewarded Overall');
		expect(overall.realisedReturns).toBe(1377);
		expect(overall.realisedRate).toBeCloseTo(1377 / 1500, 5);
	});

	it('merges market opportunity and realised markup into target rows', async () => {
		const model = createHuntingModel();
		await model.loadData();

		const atrox = required(
			model.targetSections.find((section) => section.mobSpecies === 'Atrox'),
			'Atrox target',
		);
		expect(required(atrox.muProjectedReturns, 'MU projection')).toBeCloseTo(156, 5);
		expect(atrox.realisedMarkup).toBeCloseTo(12, 5);
		expect(atrox.realisedReturns).toBeCloseTo(822, 5);
		expect(atrox.realisedRate).toBeCloseTo(822 / 900, 5);
		expect(model.targetSections.at(-1)?.label).toBe('Unclassified');
	});

	it('reconciles Overall market figures with the target rows', async () => {
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

	it('uses Overall by default and returns there for a stale session selection', async () => {
		const model = createHuntingModel();
		await model.loadData();

		expect(model.selectedSession).toBeNull();
		model.selectSession('definition:1');
		expect(required(model.selectedSession, 'selected session').definitionId).toBe(1);
		model.selectSession('definition:99');
		expect(model.selectedSession).toBeNull();
		model.selectSession(null);
		expect(model.selectedSession).toBeNull();
	});

	it('stamps hunting on listings and conversions', async () => {
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

	it('keeps the economic spine available when optional market context fails', async () => {
		mocked.getMarketHuntMarkups.mockRejectedValue(new Error('offline'));
		mocked.getHuntingRealisedMarkup.mockRejectedValue(new Error('offline'));
		const model = createHuntingModel();
		await model.loadData();
		expect(model.error).toBeNull();
		const atrox = required(
			model.targetSections.find((section) => section.mobSpecies === 'Atrox'),
			'Atrox target',
		);
		expect(atrox.muProjectedReturns).toBeNull();
		expect(atrox.realisedMarkup).toBe(0);

		mocked.getAnalyticsHuntingActivity.mockRejectedValue(new Error('backend unreachable'));
		const broken = createHuntingModel();
		await broken.loadData();
		expect(broken.error).toContain('backend unreachable');
	});
});
