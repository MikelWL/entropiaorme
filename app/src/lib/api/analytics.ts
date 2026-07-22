/**
 * The analytics family: the Overview, Hunting, and Tree Cutting aggregates, the
 * ledger (entries and presets), and the inventory ledger. Thin
 * wrappers over the generated typed commands; the reads swap onto the
 * parallel `demo_*` commands while the guide is active (see `./guide`).
 */

import type { LedgerItem } from './commands.gen';
import * as commands from './commands.gen';
import { guideSwapped } from './guide';

const readOverview = guideSwapped(commands.analyticsOverview, commands.demoAnalyticsOverview);

export async function getAnalyticsOverview(period: string = 'all') {
	return readOverview(period);
}

/** The whole-ledger per-tag summary for a period, independent of the
 * paginated entry list: the Net Ledger Impact card's source of truth. */
export const getLedgerSummary = guideSwapped(commands.ledgerSummary, commands.demoLedgerSummary);

export const getAnalyticsHunting = guideSwapped(
	commands.analyticsHunting,
	commands.demoAnalyticsHunting,
);
export const getAnalyticsHarvest = guideSwapped(
	commands.analyticsHarvest,
	commands.demoAnalyticsHarvest,
);
export const getLedgerPresets = guideSwapped(
	commands.ledgerPresetsList,
	commands.demoLedgerPresetsList,
);
export const getInventoryItems = guideSwapped(commands.inventoryList, commands.demoInventoryList);

export const addLedgerEntry = commands.ledgerCreate;
export const deleteLedgerEntry = commands.ledgerDelete;
export const addLedgerPreset = commands.ledgerPresetCreate;
export const deleteLedgerPreset = commands.ledgerPresetDelete;
// The harvest-stock removed overlay (per-item quantity already sold or
// spent): operational position context for sale and recycling actions.
// It does not influence holding-independent market opportunity. No demo
// variant; the reader degrades to an empty overlay in guide mode.
export const getHarvestStock = commands.harvestStock;
export const setHarvestStock = commands.harvestStockSet;

export const addInventoryItem = commands.inventoryCreate;
export const updateInventoryItem = commands.inventoryUpdate;
export const deleteInventoryItem = commands.inventoryDelete;
export const sellInventoryItem = commands.inventorySell;

/** One keyset page of ledger entries plus the cursor for the next page
 * (null on the last page) and the whole-ledger row count. Frontend-owned
 * reshape of the generated `LedgerPage` (`entries` reads as `items` at
 * the consumer). */
export interface LedgerPage {
	items: LedgerItem[];
	nextCursor: string | null;
	total: number;
}

const readLedgerPage = guideSwapped(commands.ledgerList, commands.demoLedgerList);

export async function getLedgerEntries(cursor?: string, limit?: number): Promise<LedgerPage> {
	const page = await readLedgerPage(cursor ?? null, limit ?? null);
	return {
		items: page.entries,
		nextCursor: page.nextCursor ?? null,
		total: page.total,
	};
}
