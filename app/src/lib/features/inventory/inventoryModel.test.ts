import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createInventoryModel } from './inventoryModel.svelte';

vi.mock('$lib/api/inventory', () => ({
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

import * as inventoryApi from '$lib/api/inventory';

const mocked = vi.mocked(inventoryApi);

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
});

describe('central inventory model', () => {
	it('loads pooled loot and equipment as separate holding families', async () => {
		const model = createInventoryModel();
		await model.load();

		expect(model.lootTt).toBe(1);
		expect(model.equipmentTt).toBe(25);
		expect(model.equipmentBasis).toBe(100);
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
});
