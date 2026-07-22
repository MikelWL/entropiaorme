/**
 * Tree Cutting-tab view model. Each harvesting tool the player has used
 * becomes its own section: a stat strip (swings, cycled, returns, rate,
 * and the markup-adjusted MU projected returns / MU rate) over a per-item
 * loot breakdown carrying each item's market markup and a liquidity
 * confidence signal.
 *
 * Two feeds compose here: the realised harvest aggregate (accounting
 * side) and the per-item market signals (the informational market
 * layer). They are merged in this frontend model; the accounting
 * boundary keeps them apart in the backend, and every MU figure is an
 * estimate, never realised P&L.
 *
 * Confidence: a markup is only realisable if the market can absorb the
 * player's position at that markup and the gain clears the auction fee.
 * Where it cannot, the realistic value is the nanocube recycling floor
 * (recycling is TT-neutral, so any item converts to nanocubes at full
 * TT and sells at the nanocube markup). A toggle sets how much
 * confidence a markup must clear before it is trusted over the floor.
 */

import {
	getAnalyticsHarvest,
	getHarvestStock,
	getMarketHarvestMarkups,
	setHarvestStock,
	type HarvestData,
	type MarketHarvestData,
	type MarketHarvestItem,
} from '$lib/api';
import type { HarvestLootItem, HarvestToolComparison } from '$lib/types/analytics';
import { describeError } from '$lib/view/errorState';

// ── Confidence model (tunable; grounded on the maintainer's data) ──────

export type ConfidenceTier = 'liquid' | 'middling' | 'illiquid';
/** How much confidence a markup must clear to be trusted over the
 * recycling floor: `liquid` trusts only liquid markups; `liquidMiddling`
 * trusts liquid + middling; `all` trusts everything at face value. */
export type ConfidenceMode = 'liquid' | 'liquidMiddling' | 'all';

const WEEKS_PER_MONTH = 4.345;
const WEEKS_PER_YEAR = 52.14;
/** Position as a fraction of weekly market throughput: at or below this
 * the market absorbs the position readily (liquid). */
const ABSORPTION_LIQUID = 0.15;
/** Above this the position is too large a share of throughput to sell at
 * markup (illiquid). Between the two is middling. */
const ABSORPTION_MIDDLING = 0.75;
/** The minimum auction fee (PED) the markup gain must clear to be worth
 * realising. TODO(ledger-market-sales): replace with the fee growth
 * curve once known. */
const AUCTION_FEE_PED = 0.5;
/** Nanocube markup fallback (percent) when the market feed carries no
 * nanocube observation. */
export const NANOCUBE_FALLBACK_MARKUP = 100.6;

const TIER_RANK: Record<ConfidenceTier, number> = { liquid: 3, middling: 2, illiquid: 1 };
const MODE_THRESHOLD: Record<ConfidenceMode, number> = { liquid: 3, liquidMiddling: 2, all: 1 };

/** The resolved horizon's sales volume normalised to a weekly rate, so
 * horizons compare (a month-resolved item is divided down, which is what
 * makes the fallback penalise its own liquidity). */
export function weeklyEquivalentVolume(
	salesPed: number | null,
	horizon: string | null,
): number {
	if (salesPed == null || salesPed <= 0) return 0;
	if (horizon === 'week') return salesPed;
	if (horizon === 'month') return salesPed / WEEKS_PER_MONTH;
	if (horizon === 'year') return salesPed / WEEKS_PER_YEAR;
	return 0;
}

/**
 * The liquidity confidence of realising an item's own market markup,
 * given the player's total position (TT). Two axes: whether the market
 * absorbs the position (absorption vs weekly throughput) and whether the
 * markup gain clears the auction fee. An uncovered item, a fallback to a
 * thinner horizon, or a fee-bound gain all sink toward illiquid.
 */
export function itemTier(
	market: MarketHarvestItem | undefined,
	positionTt: number,
): ConfidenceTier {
	if (!market || market.markupPct == null || market.horizon == null) return 'illiquid';
	const weekly = weeklyEquivalentVolume(market.salesPed, market.horizon);
	if (weekly <= 0) return 'illiquid';
	const feeProfit = positionTt * (market.markupPct / 100 - 1);
	if (feeProfit < AUCTION_FEE_PED) return 'illiquid';
	const absorption = positionTt / weekly;
	if (market.horizon === 'week' && absorption <= ABSORPTION_LIQUID) return 'liquid';
	if (market.horizon !== 'year' && absorption <= ABSORPTION_MIDDLING) return 'middling';
	return 'illiquid';
}

/**
 * The markup actually applied to an item under the current confidence
 * mode: its own market markup when the tier clears the mode's threshold,
 * else the nanocube recycling floor. `floored` says the own markup was
 * substituted (struck through in the UI).
 */
export function effectiveMarkup(
	tier: ConfidenceTier,
	ownMarkupPct: number | null,
	nanocubeMarkupPct: number,
	mode: ConfidenceMode,
): { markupPct: number; floored: boolean } {
	const trusts = TIER_RANK[tier] >= MODE_THRESHOLD[mode];
	if (trusts && ownMarkupPct != null) return { markupPct: ownMarkupPct, floored: false };
	return { markupPct: nanocubeMarkupPct, floored: true };
}

// ── Section derivation ─────────────────────────────────────────────────

const BOARD_TO_TREE: Record<string, string> = {
	'Short Moonleaf Board': 'Small',
	'Moonleaf Board': 'Long',
	'Long Moonleaf Board': 'Huge',
};

export type TreeCuttingItem = {
	name: string;
	quantity: number;
	ttValue: number;
	sharePct: number;
	/** The item's own estimated market markup (percent), or null when no
	 * observation covers it. */
	ownMarkupPct: number | null;
	markupHorizon: string | null;
	tier: ConfidenceTier;
	/** The markup applied under the current mode (own or nanocube floor). */
	effectiveMarkupPct: number;
	/** True when the own markup was replaced by the recycling floor
	 * (struck through in the UI). */
	floored: boolean;
	// Tooltip inputs.
	positionTt: number;
	/** Raw sales volume (PED) at the resolved horizon, un-normalised. */
	salesPed: number | null;
	/** Raw week-horizon sales volume (PED); 0 means the item did not sell
	 * at all last week, the signal the tooltip leads with on a fallback. */
	weeklySalesPed: number | null;
};

/** The combined stat line across every tool (the "Overall" block): the
 * same five stats, weighted by volume where they are rates. The market
 * figures respect the active confidence mode via the per-section sums. */
export type TreeCuttingOverall = {
	cycled: number;
	returns: number;
	lootRate: number;
	muProjectedReturns: number | null;
	muRate: number | null;
};

export type TreeCuttingSection = {
	toolName: string;
	tree: string | null;
	swings: number;
	cycled: number;
	returns: number;
	lootRate: number;
	/** Whole-pool markup-projected returns (PED) under the current mode,
	 * or null when the market feed is unavailable. Estimated only. */
	muProjectedReturns: number | null;
	muRate: number | null;
	items: TreeCuttingItem[];
};

export function primaryTree(items: HarvestLootItem[]): string | null {
	let best: { tree: string; tt: number } | null = null;
	for (const item of items) {
		const tree = BOARD_TO_TREE[item.itemName];
		if (tree && (!best || item.valuePed > best.tt)) {
			best = { tree, tt: item.valuePed };
		}
	}
	return best?.tree ?? null;
}

/** Lifetime recorded harvest per item across every tool: the quantity
 * and TT value looted. This is the ground-truth base the stock overlay
 * sits on. */
type LootedItem = { quantity: number; valuePed: number };
function lootedByItem(tools: HarvestToolComparison[]): Map<string, LootedItem> {
	const looted = new Map<string, LootedItem>();
	for (const tool of tools) {
		for (const item of tool.lootItems) {
			const prev = looted.get(item.itemName) ?? { quantity: 0, valuePed: 0 };
			looted.set(item.itemName, {
				quantity: prev.quantity + item.quantity,
				valuePed: prev.valuePed + item.valuePed,
			});
		}
	}
	return looted;
}

/** One horizon's market reading (day/week/month/year) for the stock row's
 * markup detail view. */
export type StockHorizonReading = {
	horizon: string;
	markupPct: number | null;
	salesPed: number;
};

/** One item's current stock: the recorded looted quantity, how much has
 * been removed (sold or spent), and the resulting held quantity and TT.
 * The held TT is the position feeding the markup-confidence check. The
 * market fields are the raw (non-confidence-adjusted) signals for the
 * markup column and its per-horizon detail view. */
export type TreeCuttingStock = {
	itemName: string;
	lootedQty: number;
	removedQty: number;
	heldQty: number;
	heldTt: number;
	/** The resolved market markup (percent): week, then month, then year;
	 * null when no observation covers the item. */
	markupPct: number | null;
	markupHorizon: string | null;
	/** The day/week/month/year breakdown for the detail view. */
	readings: StockHorizonReading[];
};

/** The held TT per item: looted TT scaled by the fraction still held
 * (looted quantity minus removed). This is the position seam: the market
 * position that gates markup confidence, kept distinct from the recorded
 * activity stats. */
function heldTtByItem(
	looted: Map<string, LootedItem>,
	removed: Map<string, number>,
): Map<string, number> {
	const held = new Map<string, number>();
	for (const [name, item] of looted) {
		const gone = Math.min(Math.max(removed.get(name) ?? 0, 0), item.quantity);
		held.set(name, item.quantity > 0 ? (item.valuePed * (item.quantity - gone)) / item.quantity : 0);
	}
	return held;
}

function toSection(
	tool: HarvestToolComparison,
	market: MarketHarvestData | null,
	marketByItem: Map<string, MarketHarvestItem>,
	position: Map<string, number>,
	mode: ConfidenceMode,
): TreeCuttingSection {
	const nanocube = market?.nanocubeMarkupPct ?? NANOCUBE_FALLBACK_MARKUP;
	const totalTt = tool.lootItems.reduce((sum, item) => sum + item.valuePed, 0);

	let muProjected = 0;
	const items: TreeCuttingItem[] = tool.lootItems.map((item) => {
		const m = marketByItem.get(item.itemName);
		const positionTt = position.get(item.itemName) ?? item.valuePed;
		const tier = itemTier(m, positionTt);
		const { markupPct: effectiveMarkupPct, floored } = effectiveMarkup(
			tier,
			m?.markupPct ?? null,
			nanocube,
			mode,
		);
		muProjected += (item.valuePed * effectiveMarkupPct) / 100;
		return {
			name: item.itemName,
			quantity: item.quantity,
			ttValue: item.valuePed,
			sharePct: totalTt > 0 ? (item.valuePed / totalTt) * 100 : 0,
			ownMarkupPct: m?.markupPct ?? null,
			markupHorizon: m?.horizon ?? null,
			tier,
			effectiveMarkupPct,
			floored,
			positionTt,
			salesPed: m?.salesPed ?? null,
				weeklySalesPed: m?.readings.find((r) => r.horizon === 'week')?.salesPed ?? null,
		};
	});

	const muProjectedReturns = market ? muProjected : null;
	const muRate =
		muProjectedReturns !== null && tool.cycled > 0 ? muProjectedReturns / tool.cycled : null;

	return {
		toolName: tool.toolName,
		tree: primaryTree(tool.lootItems),
		swings: tool.swings,
		cycled: tool.cycled,
		returns: tool.returns,
		lootRate: tool.lootRate,
		muProjectedReturns,
		muRate,
		items,
	};
}

export function createTreeCuttingModel() {
	let data = $state<HarvestData | null>(null);
	let market = $state<MarketHarvestData | null>(null);
	// The removed overlay (item -> quantity sold/spent), the market-position
	// lever behind markup confidence. Reassigned wholesale so derivations
	// re-run.
	let removed = $state<Map<string, number>>(new Map());
	let loading = $state(true);
	let error = $state<string | null>(null);
	let confidenceMode = $state<ConfidenceMode>('liquidMiddling');

	async function loadData() {
		loading = true;
		error = null;
		try {
			// The realised aggregate is the spine; the market feed and the
			// stock overlay are best-effort context, so either failing
			// degrades gracefully (MU figures blank / no removals) rather
			// than blanking the tab.
			const [harvest, markets, stock] = await Promise.all([
				getAnalyticsHarvest(),
				getMarketHarvestMarkups().catch(() => null),
				getHarvestStock().catch(() => []),
			]);
			data = harvest;
			market = markets;
			removed = new Map(stock.map((r) => [r.itemName, r.removedQty]));
		} catch (e) {
			error = describeError(e, 'Failed to load tree cutting data');
		} finally {
			loading = false;
		}
	}

	const sections = $derived.by<TreeCuttingSection[]>(() => {
		if (!data) return [];
		const marketByItem = new Map((market?.items ?? []).map((item) => [item.itemName, item]));
		// Position feeding markup confidence is the current held TT, not the
		// lifetime looted TT: selling stock changes the position without
		// touching the recorded activity stats.
		const position = heldTtByItem(lootedByItem(data.toolComparisons), removed);
		return data.toolComparisons.map((tool) =>
			toSection(tool, market, marketByItem, position, confidenceMode),
		);
	});

	/** The current stock line for the Overall block: per-item held quantity
	 * and TT, ordered by stock TT (most-held first), since market position
	 * is about TT value, not item count. Its held TT is what markup
	 * confidence uses. */
	const stock = $derived.by<TreeCuttingStock[]>(() => {
		if (!data) return [];
		const looted = lootedByItem(data.toolComparisons);
		const marketByItem = new Map((market?.items ?? []).map((item) => [item.itemName, item]));
		const rows = [...looted.entries()].map(([itemName, item]) => {
			const gone = Math.min(Math.max(removed.get(itemName) ?? 0, 0), item.quantity);
			const heldQty = item.quantity - gone;
			const unitTt = item.quantity > 0 ? item.valuePed / item.quantity : 0;
			const m = marketByItem.get(itemName);
			return {
				itemName,
				lootedQty: item.quantity,
				removedQty: gone,
				heldQty,
				heldTt: heldQty * unitTt,
				markupPct: m?.markupPct ?? null,
				markupHorizon: m?.horizon ?? null,
				readings: (m?.readings ?? []).map((r) => ({
					horizon: r.horizon,
					markupPct: r.markupPct,
					salesPed: r.salesPed,
				})),
			};
		});
		rows.sort((a, b) => b.heldTt - a.heldTt || a.itemName.localeCompare(b.itemName));
		return rows;
	});

	/** Set an item's currently-held quantity (clamped to [0, looted]); we
	 * persist the derived removed quantity. Optimistic: the local overlay
	 * updates immediately, then the write lands. */
	async function setHeld(itemName: string, heldQty: number) {
		if (!data) return;
		const looted = lootedByItem(data.toolComparisons).get(itemName);
		if (!looted) return;
		const held = Math.min(Math.max(Math.floor(heldQty), 0), looted.quantity);
		const removedQty = looted.quantity - held;
		const next = new Map(removed);
		if (removedQty > 0) next.set(itemName, removedQty);
		else next.delete(itemName);
		removed = next;
		try {
			await setHarvestStock({ itemName, removedQty });
		} catch (e) {
			error = describeError(e, 'Failed to update stock');
		}
	}

	const overall = $derived.by<TreeCuttingOverall | null>(() => {
		if (sections.length === 0) return null;
		const cycled = sections.reduce((sum, s) => sum + s.cycled, 0);
		const returns = sections.reduce((sum, s) => sum + s.returns, 0);
		const lootRate = cycled > 0 ? returns / cycled : 0;
		// Market figures aggregate only when at least one section carries
		// them; a section without market context contributes nothing.
		const anyMarket = sections.some((s) => s.muProjectedReturns !== null);
		const muProjectedReturns = anyMarket
			? sections.reduce((sum, s) => sum + (s.muProjectedReturns ?? 0), 0)
			: null;
		const muRate =
			muProjectedReturns !== null && cycled > 0 ? muProjectedReturns / cycled : null;
		return { cycled, returns, lootRate, muProjectedReturns, muRate };
	});

	return {
		get data() {
			return data;
		},
		get overall() {
			return overall;
		},
		get loading() {
			return loading;
		},
		get error() {
			return error;
		},
		get sections() {
			return sections;
		},
		get stock() {
			return stock;
		},
		get confidenceMode() {
			return confidenceMode;
		},
		set confidenceMode(value: ConfidenceMode) {
			confidenceMode = value;
		},
		loadData,
		setHeld,
	};
}

export type TreeCuttingModel = ReturnType<typeof createTreeCuttingModel>;
