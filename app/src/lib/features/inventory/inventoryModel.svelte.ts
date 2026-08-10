import type {
	ActivityHistoryEntry,
	AuctionListing,
	EquipmentListingInput,
	EquipmentTradeInput,
	InventoryItem,
	InventorySaleDraft,
	StockPosition,
} from '$lib/api/commands.gen';
import {
	commitInventorySaleDraft,
	confirmInventoryListing,
	convertLoot,
	convertLootShrapnel,
	deleteEquipmentHolding,
	expireInventoryListing,
	getEquipmentInventory,
	getInventoryHistory,
	getInventoryListings,
	getLootInventory,
	removeLoot,
	revertInventorySale,
	undoInventoryConversion,
	undoInventoryListing,
	undoInventoryRemoval,
	undoInventoryTrade,
} from '$lib/api/inventory';
import type {
	ActivityListingDraft,
	ActivityTradeDraft,
	TreeCuttingStock,
} from '$lib/features/analytics/treeCuttingModel.svelte';
import { describeError } from '$lib/view/errorState';

export type InventoryKind = 'loot' | 'equipment';
export type InventoryView = 'holdings' | 'listings' | 'history';

function stockRow(row: StockPosition): TreeCuttingStock {
	return {
		itemName: row.itemName,
		heldQty: row.quantity,
		heldTt: row.ttValue,
		listedQty: row.listedQuantity,
		readings: [],
		opportunity: null,
		markupPct: null,
		markupHorizon: null,
		tier: null,
		effectiveMarkupPct: null,
		floored: false,
		salesPed: null,
		weeklySalesPed: null,
	};
}

function manualDraft(
	channel: 'auction' | 'trade',
	name: string,
	quantity: number,
	values: {
		startingBid?: number;
		buyout?: number | null;
		listingFee?: number;
		finalPrice?: number;
	},
): InventorySaleDraft {
	return {
		draftId: crypto.randomUUID(),
		source: 'manual',
		channel,
		observedName: name,
		quantity,
		startingBid: values.startingBid ?? null,
		buyout: values.buyout ?? null,
		listingFee: values.listingFee ?? null,
		finalPrice: values.finalPrice ?? null,
		confidence: null,
	};
}

export function createInventoryModel() {
	let kind = $state<InventoryKind>('loot');
	let view = $state<InventoryView>('holdings');
	let loading = $state(true);
	let historyLoading = $state(false);
	let historyLoaded = $state(false);
	let error = $state<string | null>(null);
	let loot = $state<TreeCuttingStock[]>([]);
	let equipment = $state<InventoryItem[]>([]);
	let listings = $state<AuctionListing[]>([]);
	let history = $state<ActivityHistoryEntry[]>([]);

	async function load() {
		loading = true;
		error = null;
		try {
			const [positions, equipmentRows, listingRows] = await Promise.all([
				getLootInventory(),
				getEquipmentInventory(),
				getInventoryListings(),
			]);
			loot = positions.map(stockRow);
			equipment = equipmentRows;
			listings = listingRows;
		} catch (cause) {
			error = describeError(cause, 'Failed to load inventory');
		} finally {
			loading = false;
		}
	}

	async function loadHistory() {
		historyLoading = true;
		error = null;
		try {
			history = await getInventoryHistory();
			historyLoaded = true;
		} catch (cause) {
			error = describeError(cause, 'Failed to load inventory history');
			throw cause;
		} finally {
			historyLoading = false;
		}
	}

	async function refresh(includeHistory = historyLoaded) {
		const [positions, equipmentRows, listingRows, historyRows] = await Promise.all([
			getLootInventory(),
			getEquipmentInventory(),
			getInventoryListings(),
			includeHistory ? getInventoryHistory() : Promise.resolve(history),
		]);
		loot = positions.map(stockRow);
		equipment = equipmentRows;
		listings = listingRows;
		history = historyRows;
	}

	async function withRefresh(work: () => Promise<unknown>, message: string) {
		error = null;
		try {
			await work();
			await refresh();
		} catch (cause) {
			error = describeError(cause, message);
			throw cause;
		}
	}

	async function listLoot(input: ActivityListingDraft) {
		const draft = manualDraft('auction', input.itemName, input.quantity, input);
		await withRefresh(
			() =>
				commitInventorySaleDraft({
					draft,
					holding: { kind: 'loot', holdingId: input.itemName, name: input.itemName, score: 100 },
					occurredAt: input.listedAt ?? null,
				}),
			'Failed to create the listing',
		);
	}

	async function sellLootByTrade(input: ActivityTradeDraft) {
		const draft = manualDraft('trade', input.itemName, input.quantity, {
			finalPrice: input.soldFor,
		});
		await withRefresh(
			() =>
				commitInventorySaleDraft({
					draft,
					holding: { kind: 'loot', holdingId: input.itemName, name: input.itemName, score: 100 },
					occurredAt: input.soldAt,
				}),
			'Failed to record the trade',
		);
	}

	async function listEquipment(input: EquipmentListingInput) {
		const item = equipment.find((row) => row.id === input.itemId);
		if (!item) throw new Error('That equipment holding is no longer available');
		const draft = manualDraft('auction', item.name, 1, input);
		await withRefresh(
			() =>
				commitInventorySaleDraft({
					draft,
					holding: { kind: 'equipment', holdingId: item.id, name: item.name, score: 100 },
					occurredAt: input.listedAt ?? null,
				}),
			'Failed to create the listing',
		);
	}

	async function sellEquipmentByTrade(input: EquipmentTradeInput) {
		const item = equipment.find((row) => row.id === input.itemId);
		if (!item) throw new Error('That equipment holding is no longer available');
		const draft = manualDraft('trade', item.name, 1, { finalPrice: input.soldFor });
		await withRefresh(
			() =>
				commitInventorySaleDraft({
					draft,
					holding: { kind: 'equipment', holdingId: item.id, name: item.name, score: 100 },
					occurredAt: input.soldAt ?? null,
				}),
			'Failed to record the trade',
		);
	}

	async function resolveListing(
		listingId: string,
		outcome:
			| { sold: true; finalPrice: number; saleFee: number; resolvedAt?: string }
			| { sold: false; resolvedAt?: string },
	) {
		await withRefresh(
			() =>
				outcome.sold
					? confirmInventoryListing({
							listingId,
							finalPrice: outcome.finalPrice,
							saleFee: outcome.saleFee,
							resolvedAt: outcome.resolvedAt ?? null,
						})
					: expireInventoryListing({ listingId, resolvedAt: outcome.resolvedAt ?? null }),
			'Failed to resolve the listing',
		);
	}

	async function undo(entry: ActivityHistoryEntry, revertSale = false) {
		await withRefresh(async () => {
			const listing = listings.find((row) => row.id === entry.id);
			if (listing || entry.subjectKind === 'equipment') {
				if (revertSale) await revertInventorySale({ id: entry.id });
				else await undoInventoryListing({ id: entry.id });
			} else if (entry.kind === 'conversion') {
				await undoInventoryConversion({ id: entry.id });
			} else if (entry.kind === 'trade') {
				await undoInventoryTrade({ id: entry.id });
			} else if (entry.kind === 'removal') {
				await undoInventoryRemoval({ id: entry.id });
			}
		}, 'Failed to undo that entry');
	}

	async function recycle(itemName: string, quantity: number) {
		await withRefresh(
			() =>
				convertLoot({
					sourceItem: itemName,
					targetItem: 'Nanocube',
					quantity,
					convertedAt: null,
				}),
			'Failed to convert the stock',
		);
	}

	async function remove(itemName: string, quantity: number) {
		await withRefresh(
			() => removeLoot({ itemName, quantity, removedAt: null }),
			'Failed to remove the stock',
		);
	}

	async function shrapnel(quantity: number) {
		await withRefresh(
			() => convertLootShrapnel({ quantity, convertedAt: null }),
			'Failed to convert the Shrapnel',
		);
	}

	async function deleteEquipment(item: InventoryItem) {
		await withRefresh(() => deleteEquipmentHolding(item.id), `Failed to remove ${item.name}`);
	}

	const visibleListings = $derived(
		listings.filter((row) =>
			kind === 'equipment' ? row.subjectKind === 'equipment' : row.subjectKind === 'loot',
		),
	);
	const visibleHistory = $derived(history.filter((row) => row.subjectKind === kind));

	return {
		get kind() {
			return kind;
		},
		set kind(value: InventoryKind) {
			kind = value;
		},
		get view() {
			return view;
		},
		set view(value: InventoryView) {
			view = value;
		},
		get loading() {
			return loading;
		},
		get historyLoading() {
			return historyLoading;
		},
		get error() {
			return error;
		},
		get loot() {
			return loot;
		},
		get equipment() {
			return equipment;
		},
		get listings() {
			return visibleListings;
		},
		get openListings() {
			return visibleListings.filter((row) => row.status === 'pending');
		},
		get resolvedListings() {
			return visibleListings.filter((row) => row.status !== 'pending');
		},
		get history() {
			return visibleHistory;
		},
		get lootTt() {
			return loot.reduce((sum, row) => sum + row.heldTt, 0);
		},
		get equipmentTt() {
			return equipment.reduce((sum, row) => sum + row.ttValue, 0);
		},
		get equipmentBasis() {
			return equipment.reduce((sum, row) => sum + row.ttValue + row.markupPaid, 0);
		},
		load,
		loadHistory,
		refresh,
		listLoot,
		sellLootByTrade,
		listEquipment,
		sellEquipmentByTrade,
		resolveListing,
		undo,
		recycle,
		remove,
		shrapnel,
		deleteEquipment,
	};
}

export type InventoryModel = ReturnType<typeof createInventoryModel>;
