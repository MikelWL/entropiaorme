/**
 * Tree Cutting-tab view model. Each harvesting tool the player has used
 * becomes its own section: a stat strip (swings, cycled, returns, rate,
 * current-market return) over a per-item loot breakdown carrying each
 * item's holding-independent market-opportunity profile.
 *
 * Two feeds compose here: the realised harvest aggregate (accounting
 * side) and the per-item market signals (the informational market
 * layer). They are merged in this frontend model; the accounting
 * boundary keeps them apart in the backend, and every MU figure is an
 * estimate, never realised P&L.
 *
 * Market opportunity is intrinsic to the observed market, not the
 * player's current holding. Markup premium, normalised turnover,
 * evidence horizon, and a fee-efficient parcel distinguish broad,
 * niche, thin, and unsupported direct markets. Unsupported items use
 * the nanocube recycling route as the conservative market floor.
 */

import {
	getAnalyticsHarvest,
	getHarvestStock,
	getMarketHarvestMarkups,
	type HarvestData,
	type MarketHarvestData,
	type MarketHarvestItem,
	setHarvestStock,
} from '$lib/api';
import type { HarvestLootItem, HarvestToolComparison } from '$lib/types/analytics';
import { describeError } from '$lib/view/errorState';
import { createTableModel } from '$lib/view/tableModel.svelte';
import { analyticsPeriod, type AnalyticsRange, isAnalyticsRange } from './analyticsRange';

// ── Holding-independent market opportunity ────────────────────────────

export type OpportunityKind = 'broad' | 'niche' | 'thin' | 'recycle';
export type ConfidenceTier = 'liquid' | 'middling' | 'illiquid';
export type ConfidenceMode = 'liquid' | 'liquidMiddling' | 'all';

const TIER_RANK: Record<ConfidenceTier, number> = { liquid: 3, middling: 2, illiquid: 1 };
const MODE_THRESHOLD: Record<ConfidenceMode, number> = { liquid: 3, liquidMiddling: 2, all: 1 };

const WEEKS_PER_MONTH = 4.345;
const WEEKS_PER_YEAR = 52.14;
/**
 * The opportunity thresholds below are provisional market heuristics.
 * Recalibrate them through dogfooding as more real items are bought and
 * sold, so the model stays aligned with the lived difficulty of realising
 * markup rather than becoming anchored to the first available examples.
 */
/** The minimum auction fee (PED) the markup gain must clear to be worth
 * realising. Replace with the exact fee curve once known. */
const AUCTION_FEE_PED = 0.5;
/** A healthy parcel keeps the minimum fee to at most 10% of its gross
 * markup. This is an evidence heuristic, not a sale recommendation. */
const HEALTHY_FEE_SHARE = 0.1;
/** A healthy parcel at or below 15% of weekly turnover is broad when the
 * markup itself is supported by weekly evidence. */
const BROAD_MAX_MARKET_SHARE = 0.15;
/** Above 75% of the turnover observed at the resolved horizon, even a
 * healthy parcel is too large for the direct market evidence to support. */
const SUPPORTED_MAX_MARKET_SHARE = 0.75;
/** A 200%+ direct market has enough unit margin to be economically
 * meaningful despite sparse cadence, so it is classified as niche. */
const NICHE_MIN_PREMIUM = 1;
/** Nanocube markup fallback (percent) when the market feed carries no
 * nanocube observation. */
export const NANOCUBE_FALLBACK_MARKUP = 100.6;

/** The resolved horizon's TT turnover normalised to a weekly rate, so
 * horizons compare without involving the player's position. */
export function weeklyEquivalentVolume(salesPed: number | null, horizon: string | null): number {
	if (salesPed == null || salesPed <= 0) return 0;
	if (horizon === 'week') return salesPed;
	if (horizon === 'month') return salesPed / WEEKS_PER_MONTH;
	if (horizon === 'year') return salesPed / WEEKS_PER_YEAR;
	return 0;
}

export type MarketOpportunity = {
	kind: OpportunityKind;
	ownMarkupPct: number | null;
	appliedMarkupPct: number;
	usesNanocube: boolean;
	horizon: string | null;
	salesPed: number | null;
	weeklySalesPed: number | null;
	/** The game's Sales PED field is TT turnover for these
	 * percentage-markup harvest items, normalised to one week. */
	weeklyEquivalentSalesPed: number;
	/** Gross direct-market premium transacted per normalised week. This is
	 * market-wide evidence, never personally capturable profit. */
	weeklyPremiumThroughput: number;
	/** TT parcel at which the minimum fee is at most 10% of gross markup. */
	efficientBatchTt: number | null;
	/** Efficient parcel as a share of turnover at the resolved horizon. */
	efficientBatchMarketShare: number | null;
	/** Efficient parcel expressed as weeks of normalised market turnover. */
	efficientBatchMarketWeeks: number | null;
};

/**
 * Classify an item's direct market without consulting personal stock.
 * Broad, niche, and thin markets all carry a real opportunity and retain
 * their own MU. Direct markets whose efficient parcel overwhelms recent
 * turnover, plus uncovered items, use the universal nanocube floor.
 */
export function marketOpportunity(
	market: MarketHarvestItem | undefined,
	nanocubeMarkupPct: number,
): MarketOpportunity {
	const ownMarkupPct = market?.markupPct ?? null;
	const horizon = market?.horizon ?? null;
	const salesPed = market?.salesPed ?? null;
	const weeklySalesPed = market?.readings.find((r) => r.horizon === 'week')?.salesPed ?? null;
	const weeklyEquivalentSalesPed = weeklyEquivalentVolume(salesPed, horizon);
	const premium = ownMarkupPct == null ? 0 : ownMarkupPct / 100 - 1;
	const efficientBatchTt = premium > 0 ? AUCTION_FEE_PED / (HEALTHY_FEE_SHARE * premium) : null;
	const efficientBatchMarketShare =
		efficientBatchTt !== null && salesPed !== null && salesPed > 0
			? efficientBatchTt / salesPed
			: null;
	const efficientBatchMarketWeeks =
		efficientBatchTt !== null && weeklyEquivalentSalesPed > 0
			? efficientBatchTt / weeklyEquivalentSalesPed
			: null;
	const weeklyPremiumThroughput = Math.max(0, premium) * weeklyEquivalentSalesPed;

	let directKind: Exclude<OpportunityKind, 'recycle'> | null = null;
	if (
		premium > 0 &&
		efficientBatchMarketShare !== null &&
		efficientBatchMarketShare <= SUPPORTED_MAX_MARKET_SHARE
	) {
		if (horizon === 'week' && efficientBatchMarketShare <= BROAD_MAX_MARKET_SHARE) {
			directKind = 'broad';
		} else if (premium >= NICHE_MIN_PREMIUM) {
			directKind = 'niche';
		} else {
			directKind = 'thin';
		}
	}

	const usesNanocube =
		directKind === null || ownMarkupPct === null || ownMarkupPct < nanocubeMarkupPct;
	const kind: OpportunityKind = usesNanocube ? 'recycle' : (directKind ?? 'thin');
	return {
		kind,
		ownMarkupPct,
		appliedMarkupPct: usesNanocube ? nanocubeMarkupPct : ownMarkupPct,
		usesNanocube,
		horizon,
		salesPed,
		weeklySalesPed,
		weeklyEquivalentSalesPed,
		weeklyPremiumThroughput,
		efficientBatchTt,
		efficientBatchMarketShare,
		efficientBatchMarketWeeks,
	};
}

/** Preserve the established High / Mid / Low volume UI over the richer
 * holding-independent opportunity model. Weekly thin markets have enough
 * observed cadence for medium confidence; fallback-horizon thin markets
 * and unsupported recycling routes remain low confidence. */
export function opportunityTier(opportunity: MarketOpportunity): ConfidenceTier {
	if (opportunity.kind === 'broad') return 'liquid';
	if (
		opportunity.kind === 'niche' ||
		(opportunity.kind === 'thin' && opportunity.horizon === 'week')
	) {
		return 'middling';
	}
	return 'illiquid';
}

/** Select the multiplier used by the MU aggregate. The user-facing
 * confidence toggle keeps its established behaviour, but eligibility is
 * now based on market-wide evidence rather than the player's holding. */
export function effectiveMarkup(
	opportunity: MarketOpportunity,
	nanocubeMarkupPct: number,
	mode: ConfidenceMode,
): { markupPct: number; floored: boolean } {
	const tier = opportunityTier(opportunity);
	const trustsTier = TIER_RANK[tier] >= MODE_THRESHOLD[mode];
	const trustsDirect = !opportunity.usesNanocube && trustsTier;
	return {
		markupPct: trustsDirect ? opportunity.appliedMarkupPct : nanocubeMarkupPct,
		floored: !trustsDirect,
	};
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
	opportunity: MarketOpportunity;
	ownMarkupPct: number | null;
	markupHorizon: string | null;
	tier: ConfidenceTier;
	effectiveMarkupPct: number;
	floored: boolean;
	salesPed: number | null;
	weeklySalesPed: number | null;
};

/** The combined stat line across every tool (the "Overall" block). */
export type TreeCuttingOverall = {
	cycled: number;
	returns: number;
	lootRate: number;
	muProjectedReturns: number | null;
	muRate: number | null;
	/** Confirmed-sale MU has not landed yet, so realised currently equals
	 * TT. Keeping this field explicit makes the recognition boundary
	 * visible and gives the sale-attribution work one honest seam. */
	realisedReturns: number;
	realisedRate: number;
};

export type TreeCuttingSection = {
	toolName: string;
	tree: string | null;
	swings: number;
	cycled: number;
	returns: number;
	lootRate: number;
	/** Present-market counterfactual over this activity's observed output
	 * composition. Holding-independent and estimated only. */
	muProjectedReturns: number | null;
	muRate: number | null;
	realisedReturns: number;
	realisedRate: number;
	items: TreeCuttingItem[];
};

export type TreeCuttingActivitySortKey = 'toolName' | 'cycled' | 'realisedRate' | 'muRate';

export function treeCuttingActivityName(section: TreeCuttingSection): string {
	return section.tree ? `${section.tree} Trees` : section.toolName;
}

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
 * Holdings are operational context only. The opportunity is the same
 * holding-independent profile used by every source activity. */
export type TreeCuttingStock = {
	itemName: string;
	lootedQty: number;
	removedQty: number;
	heldQty: number;
	heldTt: number;
	/** The day/week/month/year breakdown for the detail view. */
	readings: StockHorizonReading[];
	opportunity: MarketOpportunity | null;
	markupPct: number | null;
	markupHorizon: string | null;
	tier: ConfidenceTier | null;
	effectiveMarkupPct: number | null;
	floored: boolean;
	salesPed: number | null;
	weeklySalesPed: number | null;
};

function toSection(
	tool: HarvestToolComparison,
	market: MarketHarvestData | null,
	marketByItem: Map<string, MarketHarvestItem>,
	confidenceMode: ConfidenceMode,
): TreeCuttingSection {
	const nanocube = market?.nanocubeMarkupPct ?? NANOCUBE_FALLBACK_MARKUP;
	const totalTt = tool.lootItems.reduce((sum, item) => sum + item.valuePed, 0);

	let marketProjected = 0;
	const items: TreeCuttingItem[] = tool.lootItems.map((item) => {
		const m = marketByItem.get(item.itemName);
		const opportunity = marketOpportunity(m, nanocube);
		const tier = opportunityTier(opportunity);
		const applied = effectiveMarkup(opportunity, nanocube, confidenceMode);
		marketProjected += (item.valuePed * applied.markupPct) / 100;
		return {
			name: item.itemName,
			quantity: item.quantity,
			ttValue: item.valuePed,
			sharePct: totalTt > 0 ? (item.valuePed / totalTt) * 100 : 0,
			opportunity,
			ownMarkupPct: opportunity.ownMarkupPct,
			markupHorizon: opportunity.horizon,
			tier,
			effectiveMarkupPct: applied.markupPct,
			floored: applied.floored,
			salesPed: opportunity.salesPed,
			weeklySalesPed: opportunity.weeklySalesPed,
		};
	});

	const muProjectedReturns = market ? marketProjected : null;
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
		realisedReturns: tool.returns,
		realisedRate: tool.lootRate,
		items,
	};
}

export function createTreeCuttingModel() {
	let data = $state<HarvestData | null>(null);
	// Current stock is a lifetime position even when the analytics evidence
	// is scoped to a shorter period.
	let stockData = $state<HarvestData | null>(null);
	let market = $state<MarketHarvestData | null>(null);
	// The removed overlay (item -> quantity sold/spent) drives current stock
	// only. Reassigned wholesale so derivations re-run without changing the
	// holding-independent activity opportunity.
	let removed = $state<Map<string, number>>(new Map());
	let loading = $state(true);
	let error = $state<string | null>(null);
	let confidenceMode = $state<ConfidenceMode>('liquidMiddling');
	let activeRange = $state<AnalyticsRange>('All Time');
	// Which sub-activity's detail is open. Keyed by tool name; null falls
	// back to the highest-volume section, so the busiest activity opens by
	// default and a stale key (a tool that dropped out of the data) degrades
	// to that same fallback rather than an empty panel.
	let selectedTool = $state<string | null>(null);

	let loadEpoch = 0;

	async function loadData(period: string = 'all') {
		const epoch = ++loadEpoch;
		loading = true;
		error = null;
		try {
			// The realised aggregate is the spine; the market feed and the
			// stock overlay are best-effort context, so either failing
			// degrades gracefully (MU figures blank / no removals) rather
			// than blanking the tab.
			const harvestRequest = getAnalyticsHarvest(period);
			const lifetimeHarvestRequest = period === 'all' ? harvestRequest : getAnalyticsHarvest('all');
			const [harvest, lifetimeHarvest, markets, stock] = await Promise.all([
				harvestRequest,
				lifetimeHarvestRequest,
				getMarketHarvestMarkups().catch(() => null),
				getHarvestStock().catch(() => []),
			]);
			if (epoch !== loadEpoch) return;
			data = harvest;
			stockData = lifetimeHarvest;
			market = markets;
			removed = new Map(stock.map((r) => [r.itemName, r.removedQty]));
		} catch (e) {
			if (epoch !== loadEpoch) return;
			error = describeError(e, 'Failed to load tree cutting data');
		} finally {
			if (epoch === loadEpoch) loading = false;
		}
	}

	const sections = $derived.by<TreeCuttingSection[]>(() => {
		if (!data) return [];
		const marketByItem = new Map((market?.items ?? []).map((item) => [item.itemName, item]));
		// Ordered by cycled volume (busiest first): this is the sub-activity
		// list order and the fallback selection, and it scales cleanly to an
		// activity with dozens of sub-activities.
		return data.toolComparisons
			.map((tool) => toSection(tool, market, marketByItem, confidenceMode))
			.sort((a, b) => b.cycled - a.cycled || a.toolName.localeCompare(b.toolName));
	});

	const activityTable = createTableModel<TreeCuttingSection>({
		rows: () => sections,
		pageSize: Number.MAX_SAFE_INTEGER,
		initialSort: { key: 'cycled', dir: 'desc' },
		defaultSortDirs: {
			toolName: 'asc',
			cycled: 'desc',
			realisedRate: 'desc',
			muRate: 'desc',
		},
		comparators: {
			toolName: (a, b) => treeCuttingActivityName(a).localeCompare(treeCuttingActivityName(b)),
		},
	});

	/** The sub-activity whose detail panel is open: the selected tool, or the
	 * highest-volume section when nothing is selected or the selection no
	 * longer resolves. */
	const selectedSection = $derived.by<TreeCuttingSection | null>(() => {
		if (sections.length === 0) return null;
		return sections.find((s) => s.toolName === selectedTool) ?? sections[0];
	});

	/** The current stock line for the Overall block: per-item held quantity
	 * and TT, ordered by stock TT (most-held first). The item's market
	 * opportunity is intrinsic and therefore does not consume held TT. */
	const stock = $derived.by<TreeCuttingStock[]>(() => {
		if (!stockData) return [];
		const looted = lootedByItem(stockData.toolComparisons);
		const marketByItem = new Map((market?.items ?? []).map((item) => [item.itemName, item]));
		const nanocube = market?.nanocubeMarkupPct ?? NANOCUBE_FALLBACK_MARKUP;
		const rows = [...looted.entries()].map(([itemName, item]) => {
			const gone = Math.min(Math.max(removed.get(itemName) ?? 0, 0), item.quantity);
			const heldQty = item.quantity - gone;
			const unitTt = item.quantity > 0 ? item.valuePed / item.quantity : 0;
			const m = marketByItem.get(itemName);
			const heldTt = heldQty * unitTt;
			const opportunity = market ? marketOpportunity(m, nanocube) : null;
			const applied = opportunity ? effectiveMarkup(opportunity, nanocube, confidenceMode) : null;
			return {
				itemName,
				lootedQty: item.quantity,
				removedQty: gone,
				heldQty,
				heldTt,
				readings: (m?.readings ?? []).map((r) => ({
					horizon: r.horizon,
					markupPct: r.markupPct,
					salesPed: r.salesPed,
				})),
				opportunity,
				markupPct: m?.markupPct ?? null,
				markupHorizon: m?.horizon ?? null,
				tier: opportunity ? opportunityTier(opportunity) : null,
				effectiveMarkupPct: applied?.markupPct ?? null,
				floored: applied?.floored ?? false,
				salesPed: m?.salesPed ?? null,
				weeklySalesPed: m?.readings.find((r) => r.horizon === 'week')?.salesPed ?? null,
			};
		});
		rows.sort((a, b) => b.heldTt - a.heldTt || a.itemName.localeCompare(b.itemName));
		return rows;
	});

	/** Set an item's currently-held quantity (clamped to [0, looted]); we
	 * persist the derived removed quantity. Optimistic: the local overlay
	 * updates immediately, then the write lands. */
	async function setHeld(itemName: string, heldQty: number) {
		if (!stockData) return;
		const looted = lootedByItem(stockData.toolComparisons).get(itemName);
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
		const muRate = muProjectedReturns !== null && cycled > 0 ? muProjectedReturns / cycled : null;
		return {
			cycled,
			returns,
			lootRate,
			muProjectedReturns,
			muRate,
			realisedReturns: returns,
			realisedRate: lootRate,
		};
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
		get confidenceMode() {
			return confidenceMode;
		},
		set confidenceMode(mode: ConfidenceMode) {
			confidenceMode = mode;
		},
		get activeRange() {
			return activeRange;
		},
		set activeRange(value: string) {
			if (isAnalyticsRange(value)) activeRange = value;
		},
		get period() {
			return analyticsPeriod(activeRange);
		},
		get sections() {
			return sections;
		},
		get activityTable() {
			return activityTable;
		},
		get selectedSection() {
			return selectedSection;
		},
		selectSection(toolName: string) {
			selectedTool = toolName;
		},
		get stock() {
			return stock;
		},
		loadData,
		setHeld,
	};
}

export type TreeCuttingModel = ReturnType<typeof createTreeCuttingModel>;
