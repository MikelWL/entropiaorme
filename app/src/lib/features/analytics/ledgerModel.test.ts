import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { InventoryItem, LedgerEntry, LedgerPreset } from '$lib/types/analytics';
import { createLedgerModel, PAGE_SIZE } from './ledgerModel.svelte';

vi.mock('$lib/api', () => ({
	getLedgerEntries: vi.fn(),
	addLedgerEntry: vi.fn(),
	deleteLedgerEntry: vi.fn(),
	getLedgerPresets: vi.fn(),
	addLedgerPreset: vi.fn(),
	deleteLedgerPreset: vi.fn(),
	getInventoryItems: vi.fn(),
	deleteInventoryItem: vi.fn(),
}));

import * as api from '$lib/api';

const mocked = vi.mocked(api);

function entry(overrides: Partial<LedgerEntry> = {}): LedgerEntry {
	return {
		id: 'e1',
		date: '2026-07-01T10:00:00.000Z',
		type: 'expense',
		description: 'L weapon purchase',
		amount: 45,
		tag: 'equipment',
		...overrides,
	};
}

function preset(overrides: Partial<LedgerPreset> = {}): LedgerPreset {
	return {
		id: 'p1',
		name: 'L weapon',
		type: 'expense',
		description: 'Weapon restock',
		amount: 45,
		tag: 'equipment',
		...overrides,
	};
}

function item(overrides: Partial<InventoryItem> = {}): InventoryItem {
	return {
		id: 'i1',
		name: 'Hedoc Mayhem, Adjusted',
		ttValue: 720,
		markupPaid: 540,
		notes: null,
		...overrides,
	} as InventoryItem;
}

function seedLoad(entries: LedgerEntry[] = [entry()], nextCursor: string | null = null) {
	mocked.getLedgerEntries.mockResolvedValue({ items: entries, nextCursor });
	mocked.getLedgerPresets.mockResolvedValue([preset()]);
}

beforeEach(() => {
	vi.clearAllMocks();
});

describe('loadAll', () => {
	it('loads the first keyset page and the presets', async () => {
		seedLoad([entry()], 'cursor-1');
		const model = createLedgerModel();
		await model.loadAll();

		expect(model.entries).toHaveLength(1);
		expect(model.presets).toHaveLength(1);
		expect(model.nextCursor).toBe('cursor-1');
		expect(model.loading).toBe(false);
		expect(model.error).toBeNull();
	});

	it('surfaces a load failure', async () => {
		mocked.getLedgerEntries.mockRejectedValue(new Error('backend unreachable'));
		mocked.getLedgerPresets.mockResolvedValue([]);
		const model = createLedgerModel();
		await model.loadAll();
		expect(model.error).toBe('backend unreachable');
		expect(model.loading).toBe(false);
	});
});

describe('loadMoreEntries', () => {
	it('appends the next keyset page and advances the cursor', async () => {
		seedLoad([entry({ id: 'e1' })], 'cursor-1');
		const model = createLedgerModel();
		await model.loadAll();

		mocked.getLedgerEntries.mockResolvedValue({
			items: [entry({ id: 'e2' })],
			nextCursor: null,
		});
		await model.loadMoreEntries();

		expect(mocked.getLedgerEntries).toHaveBeenLastCalledWith('cursor-1');
		expect(model.entries.map((e) => e.id)).toEqual(['e1', 'e2']);
		expect(model.nextCursor).toBeNull();
	});

	it('is a no-op when the cursor is exhausted', async () => {
		seedLoad([entry()], null);
		const model = createLedgerModel();
		await model.loadAll();
		mocked.getLedgerEntries.mockClear();

		await model.loadMoreEntries();
		expect(mocked.getLedgerEntries).not.toHaveBeenCalled();
	});

	it('surfaces a load-more failure and keeps the loaded window', async () => {
		seedLoad([entry()], 'cursor-1');
		const model = createLedgerModel();
		await model.loadAll();

		mocked.getLedgerEntries.mockRejectedValue(new Error('timed out'));
		await model.loadMoreEntries();
		expect(model.error).toBe('timed out');
		expect(model.entries).toHaveLength(1);
		expect(model.nextCursor).toBe('cursor-1');
	});
});

describe('paging', () => {
	it('pages five entries per page and resets to the first page on add', async () => {
		seedLoad(
			Array.from({ length: 12 }, (_, i) => entry({ id: `e${i}` })),
			null,
		);
		const model = createLedgerModel();
		await model.loadAll();

		expect(PAGE_SIZE).toBe(5);
		expect(model.table.totalPages).toBe(3);
		model.table.page = 2;
		expect(model.table.pageRows.map((e) => e.id)).toEqual(['e10', 'e11']);

		mocked.addLedgerEntry.mockResolvedValue(entry({ id: 'new' }));
		model.entryDescription = 'Fresh entry';
		model.entryTag = 'other';
		model.entryAmount = 3;
		await model.addEntry();

		expect(model.table.page).toBe(0);
		expect(model.entries[0].id).toBe('new');
	});
});

describe('addEntry', () => {
	it('rejects an incomplete form without calling the API', async () => {
		seedLoad();
		const model = createLedgerModel();
		await model.loadAll();

		model.entryDescription = ' ';
		model.entryTag = 'other';
		model.entryAmount = 3;
		await model.addEntry();
		expect(mocked.addLedgerEntry).not.toHaveBeenCalled();
	});

	it('clears the form and closes the modal on success', async () => {
		seedLoad();
		mocked.addLedgerEntry.mockResolvedValue(entry({ id: 'new' }));
		const model = createLedgerModel();
		await model.loadAll();

		model.showAddModal = true;
		model.entryDescription = 'Fresh entry';
		model.entryTag = 'other';
		model.entryAmount = 3;
		await model.addEntry();

		expect(model.entryDescription).toBe('');
		expect(model.entryTag).toBe('');
		expect(model.entryAmount).toBe(0);
		expect(model.showAddModal).toBe(false);
	});

	it('surfaces a failure and clears a stale error on entry', async () => {
		seedLoad();
		mocked.addLedgerEntry.mockRejectedValueOnce(new Error('rejected'));
		const model = createLedgerModel();
		await model.loadAll();

		model.entryDescription = 'Fresh entry';
		model.entryTag = 'other';
		model.entryAmount = 3;
		await model.addEntry();
		expect(model.error).toBe('rejected');

		mocked.addLedgerEntry.mockResolvedValue(entry({ id: 'new' }));
		await model.addEntry();
		expect(model.error).toBeNull();
	});
});

describe('presets', () => {
	it('applies a preset as a new entry and resets the pager', async () => {
		seedLoad(
			Array.from({ length: 6 }, (_, i) => entry({ id: `e${i}` })),
			null,
		);
		mocked.addLedgerEntry.mockResolvedValue(entry({ id: 'from-preset' }));
		const model = createLedgerModel();
		await model.loadAll();
		model.table.page = 1;

		await model.applyPreset(preset());
		expect(model.entries[0].id).toBe('from-preset');
		expect(model.table.page).toBe(0);
		expect(model.showAddModal).toBe(false);
	});

	it('saves and removes presets', async () => {
		seedLoad();
		mocked.addLedgerPreset.mockResolvedValue(preset({ id: 'p2', name: 'Repairs' }));
		mocked.deleteLedgerPreset.mockResolvedValue(undefined);
		const model = createLedgerModel();
		await model.loadAll();

		model.presetName = 'Repairs';
		model.presetDescription = 'Armour repairs';
		model.presetTag = 'repair';
		model.presetAmount = 5;
		await model.savePreset();
		expect(model.presets.map((p) => p.id)).toEqual(['p1', 'p2']);
		expect(model.presetName).toBe('');

		await model.removePreset('p1');
		expect(model.presets.map((p) => p.id)).toEqual(['p2']);
	});
});

describe('tag suggestions', () => {
	it('suggests same-type tags by frequency, excluding exact matches, only while focused', async () => {
		seedLoad(
			[
				entry({ id: 'e1', tag: 'equipment' }),
				entry({ id: 'e2', tag: 'equipment' }),
				entry({ id: 'e3', tag: 'enhancers' }),
				entry({ id: 'e4', type: 'markup', tag: 'estate' }),
			],
			null,
		);
		const model = createLedgerModel();
		await model.loadAll();

		model.entryTag = 'e';
		expect(model.ledgerTagSuggestions).toEqual([]);

		model.tagInputFocused = true;
		expect(model.ledgerTagSuggestions).toEqual(['equipment', 'enhancers']);

		model.entryTag = 'equipment';
		expect(model.ledgerTagSuggestions).toEqual([]);

		model.applyTagSuggestion('enhancers');
		expect(model.entryTag).toBe('enhancers');
		expect(model.tagInputFocused).toBe(false);
	});
});

describe('net-range summaries', () => {
	it('sums expenses and markup into the net, filtered by the active range', async () => {
		const now = Date.now();
		const recent = new Date(now - 5 * 24 * 60 * 60 * 1000).toISOString();
		const old = new Date(now - 200 * 24 * 60 * 60 * 1000).toISOString();
		seedLoad(
			[
				entry({ id: 'e1', type: 'expense', amount: 40, date: recent }),
				entry({ id: 'e2', type: 'markup', amount: 100, date: recent, tag: 'item_sale' }),
				entry({ id: 'e3', type: 'expense', amount: 10, date: old }),
			],
			null,
		);
		const model = createLedgerModel();
		await model.loadAll();

		expect(model.totalExpenses).toBe(50);
		expect(model.totalMarkup).toBe(100);
		expect(model.netLedger).toBe(50);
		expect(model.expenseTags).toEqual([{ tag: 'equipment', total: 50 }]);

		model.netRange = '30d';
		expect(model.totalExpenses).toBe(40);
		expect(model.netLedger).toBe(60);
	});
});

describe('inventory', () => {
	it('loads items and derives the TT and cost-basis totals', async () => {
		mocked.getInventoryItems.mockResolvedValue([
			item(),
			item({ id: 'i2', ttValue: 80, markupPaid: 20 }),
		]);
		const model = createLedgerModel();
		await model.loadInventory();

		expect(model.inventoryItems).toHaveLength(2);
		expect(model.inventoryTtTotal).toBe(800);
		expect(model.inventoryPaidTotal).toBe(1360);
	});

	it('upserts on save and drops the sold item', async () => {
		mocked.getInventoryItems.mockResolvedValue([item()]);
		const model = createLedgerModel();
		await model.loadInventory();

		model.handleInventorySaved(item({ id: 'i1', ttValue: 700 }));
		expect(model.inventoryItems[0].ttValue).toBe(700);

		model.handleInventorySaved(item({ id: 'i2' }));
		expect(model.inventoryItems.map((i) => i.id)).toEqual(['i2', 'i1']);

		model.handleInventorySold({ soldItem: item({ id: 'i1' }), ledgerEntry: null });
		expect(model.inventoryItems.map((i) => i.id)).toEqual(['i2']);
		expect(model.inventorySellTarget).toBeNull();
	});

	it('surfaces a delete failure and clears a stale inventory error on entry', async () => {
		mocked.getInventoryItems.mockResolvedValue([item()]);
		mocked.deleteInventoryItem.mockRejectedValueOnce(new Error('locked'));
		const model = createLedgerModel();
		await model.loadInventory();

		await model.handleInventoryDelete(item());
		expect(model.inventoryError).toBe('locked');
		expect(model.inventoryItems).toHaveLength(1);

		mocked.deleteInventoryItem.mockResolvedValue(undefined);
		await model.handleInventoryDelete(item());
		expect(model.inventoryError).toBeNull();
		expect(model.inventoryItems).toHaveLength(0);
	});
});

describe('guide demo handlers', () => {
	it('opens the sell modal by item name with an optional prefilled price', async () => {
		mocked.getInventoryItems.mockResolvedValue([item()]);
		const model = createLedgerModel();
		await model.loadInventory();

		model.openInventorySellByName('Unknown Item', 10);
		expect(model.inventorySellTarget).toBeNull();

		model.openInventorySellByName('Hedoc Mayhem, Adjusted', 1360);
		expect(model.inventorySellTarget?.id).toBe('i1');
		expect(model.inventorySellPrefilledPrice).toBe(1360);

		model.closeInventorySell();
		expect(model.inventorySellTarget).toBeNull();
		expect(model.inventorySellPrefilledPrice).toBeNull();
	});

	it('injects the synthetic sale entry once, resets the pager, and clears it', async () => {
		seedLoad(
			Array.from({ length: 6 }, (_, i) => entry({ id: `e${i}` })),
			null,
		);
		const model = createLedgerModel();
		await model.loadAll();
		model.table.page = 1;

		model.injectDemoSaleEntry('Hedoc Mayhem, Adjusted', 100);
		model.injectDemoSaleEntry('Hedoc Mayhem, Adjusted', 100);
		expect(model.entries.filter((e) => e.id === 'demo-inventory-sale')).toHaveLength(1);
		expect(model.entries[0].description).toBe('Sold Hedoc Mayhem, Adjusted at +100 PED over basis');
		expect(model.table.page).toBe(0);

		model.clearDemoSaleEntry();
		expect(model.entries.some((e) => e.id === 'demo-inventory-sale')).toBe(false);
	});
});
