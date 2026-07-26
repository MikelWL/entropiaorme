/**
 * Tree Cutting-tab view model. Durable effective yield tiers are the
 * source activities; harvesting tools are nested execution strategies.
 * Each tier carries its realised TT and holding-independent market
 * opportunity over the board composition actually extracted.
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
	confirmAuctionListing,
	convertStock,
	createAuctionListing,
	expireAuctionListing,
	getAnalyticsHarvest,
	getAuctionListings,
	getHarvestRealisedMarkup,
	getHarvestStock,
	getMarketHarvestMarkups,
	type HarvestData,
	type MarketHarvestData,
	type MarketHarvestItem,
} from '$lib/api';
import type {
	AuctionListing,
	AuctionListingInput,
	HarvestLootItem,
	HarvestTierComparison,
	HarvestYieldTier,
	RealisedTierMarkup,
	StockPosition,
} from '$lib/types/analytics';
import { describeError } from '$lib/view/errorState';
import { createTableModel } from '$lib/view/tableModel.svelte';
import { type AnalyticsRange, analyticsPeriod, isAnalyticsRange } from './analyticsRange';

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

/** The canonical item stock recycles into, at 1:1 TT. */
export const NANOCUBE_ITEM = 'Nanocube';

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

const TIER_LABEL: Record<HarvestYieldTier, string> = {
	short: 'Short Boards',
	long: 'Boards',
	huge: 'Long Boards',
	unknown: 'Unclassified',
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
	/** Markup confirmed sales have realised, already inside `realisedReturns`. */
	realisedMarkup: number;
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
	/** Markup confirmed sales of this tier's output have realised, already
	 * inside `realisedReturns`. Zero until a sale is confirmed. */
	realisedMarkup: number;
	yieldTier: HarvestYieldTier;
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

export type TreeCuttingActivitySortKey = 'yieldTier' | 'cycled' | 'realisedRate' | 'muRate';

export function treeCuttingActivityName(section: TreeCuttingSection): string {
	return TIER_LABEL[section.yieldTier];
}

export function harvestTierLabel(tier: HarvestYieldTier): string {
	return TIER_LABEL[tier];
}

/** One horizon's market reading (day/week/month/year) for the stock row's
 * markup detail view. */
export type StockHorizonReading = {
	horizon: string;
	markupPct: number | null;
	salesPed: number;
};

/** One item's current stock: what the player holds now, after everything
 * that has left through a listing or a conversion and back through an
 * expiry. Holdings are operational context only. The opportunity is the
 * same holding-independent profile used by every source activity. */
export type TreeCuttingStock = {
	itemName: string;
	heldQty: number;
	heldTt: number;
	/** Out on an unresolved auction: gone from `heldQty`, but coming back
	 * if the listing expires, so it is shown rather than silently absent. */
	listedQty: number;
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

function projectLoot(
	lootItems: HarvestLootItem[],
	cycled: number,
	market: MarketHarvestData | null,
	marketByItem: Map<string, MarketHarvestItem>,
	confidenceMode: ConfidenceMode,
): {
	items: TreeCuttingItem[];
	muProjectedReturns: number | null;
	muRate: number | null;
} {
	const nanocube = market?.nanocubeMarkupPct ?? NANOCUBE_FALLBACK_MARKUP;
	const totalTt = lootItems.reduce((sum, item) => sum + item.valuePed, 0);

	let marketProjected = 0;
	const items: TreeCuttingItem[] = lootItems.map((item) => {
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
	const muRate = muProjectedReturns !== null && cycled > 0 ? muProjectedReturns / cycled : null;
	return { items, muProjectedReturns, muRate };
}

function toSection(
	tier: HarvestTierComparison,
	market: MarketHarvestData | null,
	marketByItem: Map<string, MarketHarvestItem>,
	confidenceMode: ConfidenceMode,
	realisedMarkup: number,
): TreeCuttingSection {
	const projection = projectLoot(tier.lootItems, tier.cycled, market, marketByItem, confidenceMode);

	return {
		yieldTier: tier.yieldTier,
		swings: tier.swings,
		cycled: tier.cycled,
		returns: tier.returns,
		lootRate: tier.lootRate,
		muProjectedReturns: projection.muProjectedReturns,
		muRate: projection.muRate,
		realisedReturns: tier.returns + realisedMarkup,
		realisedRate: tier.cycled > 0 ? (tier.returns + realisedMarkup) / tier.cycled : 0,
		realisedMarkup,
		items: projection.items,
	};
}

export function createTreeCuttingModel() {
	let data = $state<HarvestData | null>(null);
	let market = $state<MarketHarvestData | null>(null);
	// Current positions, the auction lifecycle over them, and the markup
	// confirmed sales have realised per tier. All three drive holdings and
	// realised figures only, never the holding-independent opportunity.
	let positions = $state<StockPosition[]>([]);
	let listings = $state<AuctionListing[]>([]);
	let realisedByTier = $state<Map<HarvestYieldTier, number>>(new Map());
	let loading = $state(true);
	let error = $state<string | null>(null);
	let confidenceMode = $state<ConfidenceMode>('liquidMiddling');
	let activeRange = $state<AnalyticsRange>('All Time');
	// Which yield activity's detail is open. Keyed by durable tier; null falls
	// back to the highest-volume section, so the busiest activity opens by
	// default and a stale key (a tier outside the current range) degrades
	// to that same fallback rather than an empty panel.
	let selectedTier = $state<HarvestYieldTier | null>(null);

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
			const [harvest, markets, stock, openListings, realised] = await Promise.all([
				getAnalyticsHarvest(period),
				getMarketHarvestMarkups().catch(() => null),
				getHarvestStock().catch(() => []),
				getAuctionListings().catch(() => []),
				getHarvestRealisedMarkup().catch(() => []),
			]);
			if (epoch !== loadEpoch) return;
			data = harvest;
			market = markets;
			positions = stock;
			listings = openListings;
			realisedByTier = new Map(realised.map((row) => [row.yieldTier, row.netMarkup]));
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
		// Ordered by cycled volume (busiest first), with the diagnostic
		// Unclassified bucket kept after the three attributable activities.
		// This is also the fallback selection order.
		return data.tierComparisons
			.map((tier) =>
				toSection(
					tier,
					market,
					marketByItem,
					confidenceMode,
					realisedByTier.get(tier.yieldTier) ?? 0,
				),
			)
			.sort(
				(a, b) =>
					Number(a.yieldTier === 'unknown') - Number(b.yieldTier === 'unknown') ||
					b.cycled - a.cycled ||
					treeCuttingActivityName(a).localeCompare(treeCuttingActivityName(b)),
			);
	});

	const activityTable = createTableModel<TreeCuttingSection>({
		rows: () => sections,
		pageSize: Number.MAX_SAFE_INTEGER,
		initialSort: { key: 'cycled', dir: 'desc' },
		defaultSortDirs: {
			yieldTier: 'asc',
			cycled: 'desc',
			realisedRate: 'desc',
			muRate: 'desc',
		},
		comparators: {
			yieldTier: (a, b) => treeCuttingActivityName(a).localeCompare(treeCuttingActivityName(b)),
		},
	});

	/** The sub-activity whose detail panel is open: the selected tool, or the
	 * highest-volume section when nothing is selected or the selection no
	 * longer resolves. */
	const selectedSection = $derived.by<TreeCuttingSection | null>(() => {
		if (sections.length === 0) return null;
		return sections.find((s) => s.yieldTier === selectedTier) ?? sections[0];
	});

	/** The current stock line for the Overall block: per-item held quantity
	 * and TT, ordered by stock TT (most-held first). The item's market
	 * opportunity is intrinsic and therefore does not consume held TT. */
	const stock = $derived.by<TreeCuttingStock[]>(() => {
		const marketByItem = new Map((market?.items ?? []).map((item) => [item.itemName, item]));
		const nanocube = market?.nanocubeMarkupPct ?? NANOCUBE_FALLBACK_MARKUP;
		const rows = positions.map((position) => {
			const m = marketByItem.get(position.itemName);
			const opportunity = market ? marketOpportunity(m, nanocube) : null;
			const applied = opportunity ? effectiveMarkup(opportunity, nanocube, confidenceMode) : null;
			return {
				itemName: position.itemName,
				heldQty: position.quantity,
				heldTt: position.ttValue,
				listedQty: position.listedQuantity,
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

	/** Unresolved auctions, oldest first: the panel is a worklist, so what
	 * has been waiting longest is what most likely needs resolving. */
	const openListings = $derived.by<AuctionListing[]>(() =>
		listings
			.filter((listing) => listing.status === 'pending')
			.slice()
			.sort((a, b) => a.listedAt.localeCompare(b.listedAt)),
	);

	/** Resolved auctions, newest first. */
	const resolvedListings = $derived.by<AuctionListing[]>(() =>
		listings
			.filter((listing) => listing.status !== 'pending')
			.slice()
			.sort((a, b) => (b.resolvedAt ?? '').localeCompare(a.resolvedAt ?? '')),
	);

	/** Re-read everything holdings-related after a write. The activity
	 * aggregates are untouched by a sale, so only the position, listing, and
	 * realised reads are re-driven. */
	async function refreshHoldings() {
		const [stock, allListings, realised] = await Promise.all([
			getHarvestStock().catch(() => []),
			getAuctionListings().catch(() => []),
			getHarvestRealisedMarkup().catch(() => []),
		]);
		positions = stock;
		listings = allListings;
		realisedByTier = new Map(realised.map((row) => [row.yieldTier, row.netMarkup]));
	}

	/** List stock on the auction. The quantity leaves holdings now and the
	 * starting-bid fee is spent now; nothing is realised until it sells. */
	async function listStock(input: AuctionListingInput) {
		try {
			await createAuctionListing(input);
			await refreshHoldings();
		} catch (e) {
			error = describeError(e, 'Failed to create the listing');
			throw e;
		}
	}

	/** Confirm a listing sold at the price it fetched, or mark it expired
	 * and returned. Either way the listing leaves the open worklist. */
	async function resolveListing(
		listingId: string,
		outcome:
			| { sold: true; finalPrice: number; saleFee: number; resolvedAt?: string }
			| { sold: false; resolvedAt?: string },
	) {
		try {
			if (outcome.sold) {
				await confirmAuctionListing({
					listingId,
					finalPrice: outcome.finalPrice,
					saleFee: outcome.saleFee,
					resolvedAt: outcome.resolvedAt ?? null,
				});
			} else {
				await expireAuctionListing({ listingId, resolvedAt: outcome.resolvedAt ?? null });
			}
			await refreshHoldings();
		} catch (e) {
			error = describeError(e, 'Failed to resolve the listing');
			throw e;
		}
	}

	/** Recycle stock into Nanocubes at 1:1 TT, carrying its activity
	 * composition forward so a later Nanocube sale still attributes back. */
	async function recycleStock(sourceItem: string, quantity: number) {
		try {
			await convertStock({
				sourceItem,
				targetItem: NANOCUBE_ITEM,
				quantity,
				convertedAt: null,
			});
			await refreshHoldings();
		} catch (e) {
			error = describeError(e, 'Failed to convert the stock');
			throw e;
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
		const realisedMarkup = sections.reduce((sum, s) => sum + s.realisedMarkup, 0);
		const realisedReturns = returns + realisedMarkup;
		return {
			cycled,
			returns,
			lootRate,
			muProjectedReturns,
			muRate,
			realisedReturns,
			realisedRate: cycled > 0 ? realisedReturns / cycled : 0,
			realisedMarkup,
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
		selectSection(yieldTier: HarvestYieldTier) {
			selectedTier = yieldTier;
		},
		get stock() {
			return stock;
		},
		get openListings() {
			return openListings;
		},
		get resolvedListings() {
			return resolvedListings;
		},
		loadData,
		listStock,
		resolveListing,
		recycleStock,
	};
}

export type TreeCuttingModel = ReturnType<typeof createTreeCuttingModel>;
