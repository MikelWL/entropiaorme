import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createInventoryModel } from './inventoryModel.svelte';

vi.mock('$lib/api/inventory', () => ({
	captureSaleWindow: vi.fn(),
	confirmInventoryListing: vi.fn(),
	commitInventorySaleDraft: vi.fn(),
	convertLoot: vi.fn(),
	convertLootShrapnel: vi.fn(),
	deleteEquipmentHolding: vi.fn(),
	expireInventoryListing: vi.fn(),
	getEquipmentInventory: vi.fn(),
	getInventoryHistory: vi.fn(),
	getInventoryListings: vi.fn(),
	getLootInventory: vi.fn(),
	removeLoot: vi.fn(),
	revertInventorySale: vi.fn(),
	undoInventoryConversion: vi.fn(),
	undoInventoryListing: vi.fn(),
	undoInventoryRemoval: vi.fn(),
	undoInventoryTrade: vi.fn(),
}));

vi.mock('$lib/api/market', () => ({
	getMarketHarvestMarkups: vi.fn(),
	getMarketHuntMarkups: vi.fn(),
}));

import * as inventoryApi from '$lib/api/inventory';
import * as marketApi from '$lib/api/market';

const mocked = vi.mocked(inventoryApi);
const mockedMarket = vi.mocked(marketApi);

beforeEach(() => {
	vi.clearAllMocks();
	mocked.getLootInventory.mockResolvedValue([
		{ itemName: 'Animal Oil Residue', quantity: 10, ttValue: 1, listedQuantity: 0 },
	]);
	mocked.getEquipmentInventory.mockResolvedValue([
		{
			id: 'equipment-1',
			name: 'Ares Ring, Improved',
			ttValue: 25,
			markupPaid: 75,
			notes: null,
			acquiredAt: '2026-08-01',
		},
	]);
	mocked.getInventoryListings.mockResolvedValue([]);
	mocked.getInventoryHistory.mockResolvedValue([]);
	mocked.commitInventorySaleDraft.mockResolvedValue(undefined);
	mockedMarket.getMarketHuntMarkups.mockResolvedValue({
		nanocubeMarkupPct: 100.6,
		items: [
			{
				itemName: 'Animal Oil Residue',
				markupPct: 125,
				horizon: 'week',
				salesPed: 1_000,
				readings: [
					{ horizon: 'day', markupPct: 126, salesPed: 100 },
					{ horizon: 'week', markupPct: 125, salesPed: 1_000 },
				],
			},
		],
	});
	mockedMarket.getMarketHarvestMarkups.mockResolvedValue({
		nanocubeMarkupPct: 100.6,
		items: [],
	});
});

describe('central inventory model', () => {
	it('loads pooled loot and equipment as separate holding families', async () => {
		const model = createInventoryModel();
		await model.load();

		expect(model.lootTt).toBe(1);
		expect(model.equipmentTt).toBe(25);
		expect(model.equipmentBasis).toBe(100);
		expect(model.loot[0]).toMatchObject({
			markupPct: 125,
			effectiveMarkupPct: 125,
			tier: 'liquid',
			markupHorizon: 'week',
		});
		expect(model.loading).toBe(false);
	});

	it('turns a manual loot listing into the same reviewed draft an OCR intake will use', async () => {
		const model = createInventoryModel();
		await model.load();

		await model.listLoot({
			itemName: 'Animal Oil Residue',
			quantity: 5,
			startingBid: 1.1,
			buyout: 1.25,
			listingFee: 0.5,
			listedAt: '2026-08-10',
		});

		expect(mocked.commitInventorySaleDraft).toHaveBeenCalledWith({
			draft: expect.objectContaining({
				source: 'manual',
				channel: 'auction',
				observedName: 'Animal Oil Residue',
				quantity: 5,
				startingBid: 1.1,
				buyout: 1.25,
				listingFee: 0.5,
			}),
			holding: {
				kind: 'loot',
				holdingId: 'Animal Oil Residue',
				name: 'Animal Oil Residue',
				score: 100,
			},
			occurredAt: '2026-08-10',
		});
	});

	it('applies the shared confidence threshold without hiding the observed markup', async () => {
		mockedMarket.getMarketHuntMarkups.mockResolvedValue({
			nanocubeMarkupPct: 100.6,
			items: [
				{
					itemName: 'Animal Oil Residue',
					markupPct: 110,
					horizon: 'month',
					salesPed: 100,
					readings: [
						{ horizon: 'week', markupPct: null, salesPed: 0 },
						{ horizon: 'month', markupPct: 110, salesPed: 100 },
					],
				},
			],
		});
		const model = createInventoryModel();
		await model.load();

		expect(model.loot[0]).toMatchObject({
			markupPct: 110,
			effectiveMarkupPct: 100.6,
			tier: 'illiquid',
			floored: true,
		});

		model.confidenceMode = 'all';
		expect(model.loot[0]).toMatchObject({
			markupPct: 110,
			effectiveMarkupPct: 110,
			floored: false,
		});
	});
});
