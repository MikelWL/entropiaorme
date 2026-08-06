/**
 * Hunting-tab view model: the session-definition and observed-target axes
 * over the same market, stock, and history machinery Tree Cutting
 * established.
 *
 * Two honest comparison axes compose here. Sessions are the routines the
 * player deliberately defined (keyed by definition, never by recorded
 * free text), each opening onto its activity signatures, quest economics,
 * mob composition, and instance trend. Targets are what the tracker
 * observed (mob species, with maturity as a drilldown), each carrying the
 * same holding-independent market opportunity over its loot composition
 * that Tree Cutting's tiers carry.
 *
 * Every accounting figure is DIRECT: weapon plus enhancer cost at kill
 * grain, loot TT, and session-grain skill. Heal and armour stay session
 * residues on Dashboard and Overview. Every MU figure is an estimate,
 * never realised P&L; realised markup arrives only through confirmed
 * sales, attributed to species through the weighted provenance of the
 * source loot.
 */

import {
	confirmAuctionListing,
	convertStock,
	createAuctionListing,
	expireAuctionListing,
	getActivityHistory,
	getActivityStock,
	getAnalyticsHuntingActivity,
	getAuctionListings,
	getHuntingRealisedMarkup,
	getMarketHuntMarkups,
	type HuntingActivityData,
	type MarketHarvestData,
	type MarketHarvestItem,
	revertAuctionSale,
	undoAuctionListing,
	undoStockConversion,
} from '$lib/api';
import type {
	ActivityHistoryEntry,
	AuctionListing,
	HuntingDefinitionComparison,
	HuntingSignature,
	HuntingSpeciesComparison,
	StockPosition,
} from '$lib/types/analytics';
import { describeError } from '$lib/view/errorState';
import { createTableModel } from '$lib/view/tableModel.svelte';
import { type AnalyticsRange, analyticsPeriod, isAnalyticsRange } from './analyticsRange';
import {
	type ActivityListingDraft,
	type TreeCuttingStock as ActivityStockRow,
	type ConfidenceMode,
	effectiveMarkup,
	marketOpportunity,
	NANOCUBE_FALLBACK_MARKUP,
	NANOCUBE_ITEM,
	opportunityTier,
	projectLoot,
	type TreeCuttingItem,
} from './treeCuttingModel.svelte';

// ── Sessions axis ──────────────────────────────────────────────────────

/** One session-definition row with its display key (definitions are keyed
 * by id; the unassigned bucket by this sentinel). */
export type HuntingSessionSection = HuntingDefinitionComparison & {
	key: string;
	isUnassigned: boolean;
	ttNet: number;
	trend: 'improving' | 'declining' | 'stable' | null;
};

export type HuntingSessionSortKey = 'name' | 'cycled' | 'lootRate' | 'pesPer100Ped';

/** Quest economics for one quest-shaped signature row: the measured
 * shortfall the fixed reward has to clear, per recorded run. A run is a
 * declared focus stretch, not a proven completion, and the readout says
 * what it measures. */
export type SignatureEconomics = {
	/** Direct TT net over the signature (negative is the ordinary case). */
	net: number;
	/** Average shortfall per run: what one run costs before its reward. */
	shortfallPerRun: number | null;
	/** The configured liquid reward per completion, when one is configured. */
	rewardPed: number | null;
	rewardIsSkill: boolean;
	/** Net after the configured liquid reward, per run. */
	netAfterRewardPerRun: number | null;
	/** The informational voucher-markup scenario on the reward, per run. */
	voucherScenarioPerRun: number | null;
};

/** Derive the break-even readout for a signature row. Skill rewards are
 * PES, never liquid, so they produce no liquid break-even line. */
export function signatureEconomics(row: HuntingSignature): SignatureEconomics {
	const net = row.returns - row.cycled;
	const runs = row.runs > 0 ? row.runs : null;
	const shortfallPerRun = runs !== null ? (row.cycled - row.returns) / runs : null;
	const rewardPed = row.rewardIsSkill ? null : (row.rewardPed ?? null);
	const netAfterRewardPerRun =
		runs !== null && rewardPed !== null ? rewardPed - (row.cycled - row.returns) / runs : null;
	const voucherScenarioPerRun =
		runs !== null && rewardPed !== null && row.expectedRewardMarkupPercent != null
			? (rewardPed * row.expectedRewardMarkupPercent) / 100 - (row.cycled - row.returns) / runs
			: null;
	return {
		net,
		shortfallPerRun,
		rewardPed,
		rewardIsSkill: row.rewardIsSkill,
		netAfterRewardPerRun,
		voucherScenarioPerRun,
	};
}

/** The definition's instance trend: the newer half of its recorded
 * instances against the older half, on the loot rate. Needs at least
 * eight instances before it says anything (loot is the noisiest series in
 * the game, and a thinner sample is one lucky loot wearing a verdict);
 * within two percentage points it reads stable. */
export function instanceTrend(
	rows: { cycled: number; returns: number }[],
): 'improving' | 'declining' | 'stable' | null {
	if (rows.length < 8) return null;
	const half = Math.floor(rows.length / 2);
	const rate = (slice: { cycled: number; returns: number }[]) => {
		const cycled = slice.reduce((sum, row) => sum + row.cycled, 0);
		const returns = slice.reduce((sum, row) => sum + row.returns, 0);
		return cycled > 0 ? returns / cycled : null;
	};
	// Instance rows arrive newest first.
	const newer = rate(rows.slice(0, half));
	const older = rate(rows.slice(rows.length - half));
	if (newer === null || older === null) return null;
	const delta = newer - older;
	if (delta > 0.02) return 'improving';
	if (delta < -0.02) return 'declining';
	return 'stable';
}

// ── Targets axis ───────────────────────────────────────────────────────

/** The label the unclassified bucket renders under. */
export const UNCLASSIFIED_LABEL = 'Unclassified';

/** One species row with the merged market layer, in the same shape the
 * Tree Cutting sub-activities carry so the two tabs read identically. */
export type HuntingTargetSection = HuntingSpeciesComparison & {
	key: string;
	label: string;
	isUnclassified: boolean;
	realisedMarkup: number;
	muProjectedReturns: number | null;
	muRate: number | null;
	realisedReturns: number;
	realisedRate: number;
	items: TreeCuttingItem[];
};

export type HuntingTargetSortKey = 'label' | 'cycled' | 'realisedRate' | 'muRate';

/** The combined direct + market stat line across the whole activity. */
export type HuntingOverallLine = {
	sessions: number;
	kills: number;
	durationHours: number;
	cycled: number;
	returns: number;
	lootRate: number;
	pes: number;
	pesPer100Ped: number;
	muProjectedReturns: number | null;
	muRate: number | null;
	realisedMarkup: number;
	realisedReturns: number;
	realisedRate: number;
	/** Confirmed markup whose species has no kills in the selected period:
	 * still real, still counted, and disclosed rather than dropped. */
	realisedOutsidePeriod: number;
};

export function createHuntingModel() {
	let data = $state<HuntingActivityData | null>(null);
	let market = $state<MarketHarvestData | null>(null);
	// Current positions, the auction lifecycle over them, and the markup
	// confirmed sales have realised per species. All three drive holdings
	// and realised figures only, never the holding-independent opportunity.
	let positions = $state<StockPosition[]>([]);
	let listings = $state<AuctionListing[]>([]);
	// Read on demand rather than with the tab: History is a surface the
	// player opens deliberately, and the verdicts on it are computed per
	// entry.
	let history = $state<ActivityHistoryEntry[]>([]);
	let realisedBySpecies = $state<Map<string, number>>(new Map());
	let loading = $state(true);
	let error = $state<string | null>(null);
	let confidenceMode = $state<ConfidenceMode>('liquidMiddling');
	let activeRange = $state<AnalyticsRange>('All Time');
	let selectedSessionKey = $state<string | null>(null);
	let selectedTargetKey = $state<string | null>(null);

	let loadEpoch = 0;

	async function loadData(period: string = 'all') {
		const epoch = ++loadEpoch;
		loading = true;
		error = null;
		try {
			// The comparison aggregate is the spine; the market feed and the
			// stock overlay are best-effort context, so either failing
			// degrades gracefully (MU figures blank / no removals) rather
			// than blanking the tab.
			const [activity, markets, stock, openListings, realised] = await Promise.all([
				getAnalyticsHuntingActivity(period),
				getMarketHuntMarkups().catch(() => null),
				getActivityStock('hunting').catch(() => []),
				getAuctionListings('hunting').catch(() => []),
				getHuntingRealisedMarkup().catch(() => []),
			]);
			if (epoch !== loadEpoch) return;
			data = activity;
			market = markets;
			positions = stock;
			listings = openListings;
			realisedBySpecies = new Map(realised.map((row) => [row.mobSpecies, row.netMarkup]));
		} catch (e) {
			if (epoch !== loadEpoch) return;
			error = describeError(e, 'Failed to load hunting data');
		} finally {
			if (epoch === loadEpoch) loading = false;
		}
	}

	// ── Sessions ──

	const sessionSections = $derived.by<HuntingSessionSection[]>(() => {
		if (!data) return [];
		return data.definitions.map((row) => ({
			...row,
			key: row.definitionId === null ? 'unassigned' : `definition:${row.definitionId}`,
			isUnassigned: row.definitionId === null,
			ttNet: row.returns - row.cycled,
			trend: instanceTrend(row.instanceRows),
		}));
	});

	const sessionTable = createTableModel<HuntingSessionSection>({
		rows: () => sessionSections,
		pageSize: Number.MAX_SAFE_INTEGER,
		searchText: (row) => [row.name],
		initialSort: { key: 'cycled', dir: 'desc' },
		defaultSortDirs: {
			name: 'asc',
			cycled: 'desc',
			lootRate: 'desc',
			pesPer100Ped: 'desc',
		},
		comparators: {
			name: (a, b) => a.name.localeCompare(b.name),
		},
	});

	/** The session whose detail panel is open: the selection, or the
	 * busiest definition when nothing is selected or the selection no
	 * longer resolves (a period switch can retire a key). */
	const selectedSession = $derived.by<HuntingSessionSection | null>(() => {
		if (sessionSections.length === 0) return null;
		return sessionSections.find((s) => s.key === selectedSessionKey) ?? sessionSections[0];
	});

	// ── Targets ──

	const targetSections = $derived.by<HuntingTargetSection[]>(() => {
		if (!data) return [];
		const marketByItem = new Map<string, MarketHarvestItem>(
			(market?.items ?? []).map((item) => [item.itemName, item]),
		);
		return data.species.map((row) => {
			const isUnclassified = row.mobSpecies === '';
			const projection = projectLoot(
				row.lootItems,
				row.cycled,
				market,
				marketByItem,
				confidenceMode,
			);
			const realisedMarkup = realisedBySpecies.get(row.mobSpecies) ?? 0;
			const realisedReturns = row.returns + realisedMarkup;
			return {
				...row,
				key: isUnclassified ? 'unclassified' : `species:${row.mobSpecies}`,
				label: isUnclassified ? UNCLASSIFIED_LABEL : row.mobSpecies,
				isUnclassified,
				realisedMarkup,
				muProjectedReturns: projection.muProjectedReturns,
				muRate: projection.muRate,
				realisedReturns,
				realisedRate: row.cycled > 0 ? realisedReturns / row.cycled : 0,
				items: projection.items,
			};
		});
	});

	const targetTable = createTableModel<HuntingTargetSection>({
		rows: () => targetSections,
		pageSize: Number.MAX_SAFE_INTEGER,
		searchText: (row) => [row.label],
		initialSort: { key: 'cycled', dir: 'desc' },
		defaultSortDirs: {
			label: 'asc',
			cycled: 'desc',
			realisedRate: 'desc',
			muRate: 'desc',
		},
		comparators: {
			label: (a, b) => a.label.localeCompare(b.label),
		},
	});

	/** The target whose detail panel is open, with the same busiest-first
	 * fallback as the sessions pane. */
	const selectedTarget = $derived.by<HuntingTargetSection | null>(() => {
		if (targetSections.length === 0) return null;
		return targetSections.find((s) => s.key === selectedTargetKey) ?? targetSections[0];
	});

	// ── Overall ──

	const overall = $derived.by<HuntingOverallLine | null>(() => {
		if (!data || data.overall.sessions === 0) return null;
		// Market figures aggregate over the species sections so the headline
		// reconciles with the rows beneath it; a section without market
		// context contributes nothing.
		const anyMarket = targetSections.some((s) => s.muProjectedReturns !== null);
		const muProjectedReturns = anyMarket
			? targetSections.reduce((sum, s) => sum + (s.muProjectedReturns ?? 0), 0)
			: null;
		const cycled = data.overall.cycled;
		const muRate = muProjectedReturns !== null && cycled > 0 ? muProjectedReturns / cycled : null;
		// Realised markup sums over EVERY species with confirmed sales, not
		// only those hunted in the selected period: the money exists either
		// way, and the remainder is disclosed rather than silently dropped.
		const realisedMarkup = [...realisedBySpecies.values()].reduce((sum, v) => sum + v, 0);
		const realisedInPeriod = targetSections.reduce((sum, s) => sum + s.realisedMarkup, 0);
		const realisedReturns = data.overall.returns + realisedMarkup;
		return {
			realisedOutsidePeriod: realisedMarkup - realisedInPeriod,
			sessions: data.overall.sessions,
			kills: data.overall.kills,
			durationHours: data.overall.durationHours,
			cycled,
			returns: data.overall.returns,
			lootRate: data.overall.lootRate,
			pes: data.overall.pes,
			pesPer100Ped: data.overall.pesPer100Ped,
			muProjectedReturns,
			muRate,
			realisedMarkup,
			realisedReturns,
			realisedRate: cycled > 0 ? realisedReturns / cycled : 0,
		};
	});

	// ── Stock (identical machinery to Tree Cutting, hunting-scoped) ──

	/** The current stock line for the Overall block: per-item held quantity
	 * and TT, ordered by stock TT (most-held first). The item's market
	 * opportunity is intrinsic and therefore does not consume held TT. */
	const stock = $derived.by<ActivityStockRow[]>(() => {
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

	/** Re-read everything holdings-related after a write. The comparison
	 * aggregates are untouched by a sale, so only the position, listing, and
	 * realised reads are re-driven. */
	async function refreshHoldings() {
		// After a write that already succeeded, a failed re-read leaves
		// figures that are known to be out of date; the last-good figures
		// stay and the tab says they are stale rather than lying either way.
		let stale = false;
		const failed = <T>(fallback: T) => {
			stale = true;
			return fallback;
		};
		const [stockRows, allListings, realised] = await Promise.all([
			getActivityStock('hunting').catch(() => failed(positions)),
			getAuctionListings('hunting').catch(() => failed(listings)),
			getHuntingRealisedMarkup().catch(() => failed(null)),
		]);
		positions = stockRows;
		listings = allListings;
		if (realised) {
			realisedBySpecies = new Map(realised.map((row) => [row.mobSpecies, row.netMarkup]));
		}
		// Only once it has been opened: an undo verdict depends on every
		// other entry, so a stale list would offer undos that no longer apply.
		if (history.length > 0) {
			history = await getActivityHistory('hunting').catch(() => failed(history));
		}
		error = stale
			? 'That went through, but the figures below could not be re-read and may be out of date.'
			: null;
	}

	/** Everything this activity has done to its stock, newest first. */
	async function loadHistory() {
		try {
			history = await getActivityHistory('hunting');
		} catch (e) {
			error = describeError(e, 'Failed to load the activity history');
			throw e;
		}
	}

	/** Take back one history entry. `revertSale` leaves the listing open
	 * instead of removing it, which only a sold listing can do. */
	async function undoHistoryEntry(entry: ActivityHistoryEntry, revertSale = false) {
		try {
			if (entry.kind === 'conversion') {
				await undoStockConversion({ id: entry.id });
			} else if (revertSale) {
				await revertAuctionSale({ id: entry.id });
			} else {
				await undoAuctionListing({ id: entry.id });
			}
			await refreshHoldings();
			history = await getActivityHistory('hunting');
		} catch (e) {
			error = describeError(e, 'Failed to undo that entry');
			throw e;
		}
	}

	/** List stock on the auction. The quantity leaves holdings now and the
	 * starting-bid fee is spent now; nothing is realised until it sells. */
	async function listStock(input: ActivityListingDraft) {
		try {
			await createAuctionListing({ profession: 'hunting', ...input });
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
				profession: 'hunting',
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
		get sessionSections() {
			return sessionSections;
		},
		get sessionTable() {
			return sessionTable;
		},
		get selectedSession() {
			return selectedSession;
		},
		selectSession(key: string) {
			selectedSessionKey = key;
		},
		get targetSections() {
			return targetSections;
		},
		get targetTable() {
			return targetTable;
		},
		get selectedTarget() {
			return selectedTarget;
		},
		selectTarget(key: string) {
			selectedTargetKey = key;
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
		get history() {
			return history;
		},
		loadHistory,
		undoHistoryEntry,
		loadData,
		listStock,
		resolveListing,
		recycleStock,
	};
}

export type HuntingModel = ReturnType<typeof createHuntingModel>;
