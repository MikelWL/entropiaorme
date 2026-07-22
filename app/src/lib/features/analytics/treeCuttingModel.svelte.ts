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
	getMarketHarvestMarkups,
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
	weeklyEquivVolume: number;
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

/** Total looted TT per item across every tool (the position for the
 * liquidity check). This is the position seam: today it is historical
 * looted TT; a future holdings primitive (looted minus sold) swaps in
 * here without touching the confidence logic. */
function positionByItem(tools: HarvestToolComparison[]): Map<string, number> {
	const position = new Map<string, number>();
	for (const tool of tools) {
		for (const item of tool.lootItems) {
			position.set(item.itemName, (position.get(item.itemName) ?? 0) + item.valuePed);
		}
	}
	return position;
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
			weeklyEquivVolume: weeklyEquivalentVolume(m?.salesPed ?? null, m?.horizon ?? null),
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
	let loading = $state(true);
	let error = $state<string | null>(null);
	let confidenceMode = $state<ConfidenceMode>('liquidMiddling');

	async function loadData() {
		loading = true;
		error = null;
		try {
			// The realised aggregate is the spine; the market feed is
			// best-effort context, so a market failure degrades to the
			// realised view (MU figures blank) rather than blanking the tab.
			const [harvest, markets] = await Promise.all([
				getAnalyticsHarvest(),
				getMarketHarvestMarkups().catch(() => null),
			]);
			data = harvest;
			market = markets;
		} catch (e) {
			error = describeError(e, 'Failed to load tree cutting data');
		} finally {
			loading = false;
		}
	}

	const sections = $derived.by<TreeCuttingSection[]>(() => {
		if (!data) return [];
		const marketByItem = new Map((market?.items ?? []).map((item) => [item.itemName, item]));
		const position = positionByItem(data.toolComparisons);
		return data.toolComparisons.map((tool) =>
			toSection(tool, market, marketByItem, position, confidenceMode),
		);
	});

	return {
		get data() {
			return data;
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
		get confidenceMode() {
			return confidenceMode;
		},
		set confidenceMode(value: ConfidenceMode) {
			confidenceMode = value;
		},
		loadData,
	};
}

export type TreeCuttingModel = ReturnType<typeof createTreeCuttingModel>;
