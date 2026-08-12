import type {
	ActivityHistoryEntry,
	AuctionListing,
	EquipmentListingInput,
	EquipmentTradeInput,
	InventoryHoldingCandidate,
	InventoryItem,
	InventorySaleDraft,
	MarketHarvestData,
	MarketHarvestItem,
	StockPosition,
} from '$lib/api/commands.gen';
import {
	captureSaleWindow,
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
	resolveInventoryDraft,
	revertInventorySale,
	takeSaleWindowCapture,
	undoInventoryConversion,
	undoInventoryListing,
	undoInventoryRemoval,
	undoInventoryTrade,
} from '$lib/api/inventory';
import { getMarketHarvestMarkups, getMarketHuntMarkups } from '$lib/api/market';
import type {
	ActivityListingDraft,
	ActivityTradeDraft,
	TreeCuttingStock,
} from '$lib/features/analytics/treeCuttingModel.svelte';
import {
	type ConfidenceMode,
	effectiveMarkup,
	marketOpportunity,
	NANOCUBE_FALLBACK_MARKUP,
	opportunityTier,
} from '$lib/features/analytics/treeCuttingModel.svelte';
import { describeError } from '$lib/view/errorState';
import type { ListingDraftFields } from './listingIntake';

export type InventoryKind = 'loot' | 'equipment';
export type InventoryView = 'holdings' | 'listings' | 'history';

function mergeMarketFeeds(feeds: Array<MarketHarvestData | null>): MarketHarvestData | null {
	const available = feeds.filter((feed): feed is MarketHarvestData => feed !== null);
	if (available.length === 0) return null;
	const items = new Map<string, MarketHarvestItem>();
	for (const feed of available) {
		for (const item of feed.items) items.set(item.itemName, item);
	}
	return {
		nanocubeMarkupPct:
			available.find((feed) => feed.nanocubeMarkupPct !== null)?.nanocubeMarkupPct ?? null,
		items: [...items.values()].sort((a, b) => a.itemName.localeCompare(b.itemName)),
	};
}

function stockRow(
	row: StockPosition,
	market: MarketHarvestData | null,
	confidenceMode: ConfidenceMode,
): TreeCuttingStock {
	const marketItem = market?.items.find((item) => item.itemName === row.itemName);
	const nanocubeMarkup = market?.nanocubeMarkupPct ?? NANOCUBE_FALLBACK_MARKUP;
	const opportunity = market ? marketOpportunity(marketItem, nanocubeMarkup) : null;
	const applied = opportunity ? effectiveMarkup(opportunity, nanocubeMarkup, confidenceMode) : null;
	return {
		itemName: row.itemName,
		heldQty: row.quantity,
		heldTt: row.ttValue,
		listedQty: row.listedQuantity,
		readings: (marketItem?.readings ?? []).map((reading) => ({
			horizon: reading.horizon,
			markupPct: reading.markupPct,
			salesPed: reading.salesPed,
		})),
		opportunity,
		markupPct: marketItem?.markupPct ?? null,
		markupHorizon: marketItem?.horizon ?? null,
		tier: opportunity ? opportunityTier(opportunity) : null,
		effectiveMarkupPct: applied?.markupPct ?? null,
		floored: applied?.floored ?? false,
		salesPed: marketItem?.salesPed ?? null,
		weeklySalesPed:
			marketItem?.readings.find((reading) => reading.horizon === 'week')?.salesPed ?? null,
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
		auctionDays: null,
		confidence: null,
	};
}

export function createInventoryModel() {
	let kind = $state<InventoryKind>('loot');
	let view = $state<InventoryView>('holdings');
	let confidenceMode = $state<ConfidenceMode>('liquidMiddling');
	let loading = $state(true);
	let historyLoading = $state(false);
	let historyLoaded = $state(false);
	let error = $state<string | null>(null);
	let positions = $state<StockPosition[]>([]);
	let market = $state<MarketHarvestData | null>(null);
	let equipment = $state<InventoryItem[]>([]);
	let listings = $state<AuctionListing[]>([]);
	let history = $state<ActivityHistoryEntry[]>([]);

	async function load() {
		loading = true;
		error = null;
		try {
			const [positionRows, equipmentRows, listingRows, huntingMarket, harvestingMarket] =
				await Promise.all([
					getLootInventory(),
					getEquipmentInventory(),
					getInventoryListings(),
					getMarketHuntMarkups().catch(() => null),
					getMarketHarvestMarkups().catch(() => null),
				]);
			positions = positionRows;
			market = mergeMarketFeeds([huntingMarket, harvestingMarket]);
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
		const [positionRows, equipmentRows, listingRows, historyRows] = await Promise.all([
			getLootInventory(),
			getEquipmentInventory(),
			getInventoryListings(),
			includeHistory ? getInventoryHistory() : Promise.resolve(history),
		]);
		positions = positionRows;
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
		if (!item) throw new Error('That asset is no longer available');
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
		if (!item) throw new Error('That asset is no longer available');
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

	/** Candidate holdings for a name read or typed off the game's sale window.
	 * Conservative by construction: a winner comes back only for a match with
	 * no plausible rival. */
	async function resolveDraftName(name: string, channel: 'auction' | 'trade') {
		const resolution = await resolveInventoryDraft({
			...manualDraft(channel, name, 0, {}),
			quantity: null,
		});
		return { candidates: resolution.candidates, resolved: resolution.resolved };
	}

	/** Commit an intake draft against the holding the user reviewed. The same
	 * boundary a captured draft will cross; nothing here is intake-specific. */
	async function createFromDraft({
		fields,
		channel,
		holding,
		occurredAt,
	}: {
		fields: ListingDraftFields;
		channel: 'auction' | 'trade';
		holding: InventoryHoldingCandidate;
		occurredAt: string | null;
	}) {
		const draft: InventorySaleDraft = {
			draftId: crypto.randomUUID(),
			source: 'manual',
			channel,
			observedName: holding.name,
			quantity: fields.quantity,
			startingBid: fields.startingBid,
			buyout: fields.buyout,
			listingFee: fields.auctionFee,
			finalPrice: channel === 'trade' ? fields.buyout : null,
			auctionDays: channel === 'auction' ? fields.auctionDays : null,
			confidence: null,
		};
		await withRefresh(
			() => commitInventorySaleDraft({ draft, holding, occurredAt }),
			channel === 'auction' ? 'Failed to create the listing' : 'Failed to record the trade',
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
	const loot = $derived.by(() =>
		positions
			.map((position) => stockRow(position, market, confidenceMode))
			.sort((a, b) => b.heldTt - a.heldTt || a.itemName.localeCompare(b.itemName)),
	);
	// Everything currently held, in one list, for the intake typeahead: the
	// sale window names an item, not a profession or an asset class.
	const holdingOptions = $derived([
		...loot.map((row) => ({
			kind: 'loot',
			holdingId: row.itemName,
			name: row.itemName,
			score: 100,
			heldQty: row.heldQty,
		})),
		...equipment.map((row) => ({
			kind: 'equipment',
			holdingId: row.id,
			name: row.name,
			score: 100,
			// A capital position is indivisible, so its whole TT is its unit.
			heldQty: 1,
		})),
	]);

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
		get confidenceMode() {
			return confidenceMode;
		},
		set confidenceMode(value: ConfidenceMode) {
			confidenceMode = value;
		},
		get loading() {
			return loading;
		},
		get historyLoading() {
			return historyLoading;
		},
		get historyLoaded() {
			return historyLoaded;
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
		get holdingOptions() {
			return holdingOptions;
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
		resolveDraftName,
		captureSaleWindow,
		takeSaleWindowCapture,
		createFromDraft,
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
