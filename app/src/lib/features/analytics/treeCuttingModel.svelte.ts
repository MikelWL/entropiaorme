/**
 * Tree Cutting-tab view model. Each harvesting tool the player has used
 * becomes its own section: a stat strip (swings, cycled, returns, rate,
 * and the markup-adjusted MU projected returns / MU rate) over a per-item
 * loot breakdown carrying each item's market markup.
 *
 * Two feeds compose here: the realised harvest aggregate (accounting
 * side) and the market tool-ranking (the informational market layer).
 * They are merged in this frontend model, keyed by tool then item; the
 * accounting boundary keeps them apart in the backend, and the MU
 * figures are always estimates, never realised P&L.
 */

import {
	getAnalyticsHarvest,
	getMarketToolRanking,
	type HarvestData,
	type MarketToolItemMarkup,
	type MarketToolRankingRow,
} from '$lib/api';
import type { HarvestLootItem, HarvestToolComparison } from '$lib/types/analytics';
import { describeError } from '$lib/view/errorState';

/**
 * The board a tool mostly pulls names the tree it mostly cuts. The three
 * tree sizes and their signature boards (per Entropia's wood loot):
 * Short Moonleaf Board from small trees, Moonleaf Board from long trees,
 * Long Moonleaf Board from huge trees.
 */
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
	/** Estimated market markup (percent), or null when no observation
	 * covers the item. */
	markupPct: number | null;
	/** The horizon that supplied the markup ('week' | 'month' | 'year'),
	 * null when uncovered. */
	markupHorizon: string | null;
};

export type TreeCuttingSection = {
	toolName: string;
	/** Inferred primary tree ('Small' | 'Long' | 'Huge'), or null when
	 * no board loot has been recorded to infer from. */
	tree: string | null;
	swings: number;
	cycled: number;
	returns: number;
	lootRate: number;
	/** Whole-pool markup-projected returns (PED), or null when the market
	 * feed has no row for the tool. Estimated: informational only. */
	muProjectedReturns: number | null;
	/** MU projected returns over cycled cost, or null when unavailable. */
	muRate: number | null;
	/** Fraction (0-1) of loot TT with a resolved markup; null when no
	 * market row. */
	coverage: number | null;
	items: TreeCuttingItem[];
};

/**
 * The primary tree a tool has mostly cut: the tree behind its
 * highest-TT board item. Null when the tool has pulled no board loot
 * (e.g. only Wood Shavings, which every tree size drops).
 */
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

function toSection(
	tool: HarvestToolComparison,
	market: MarketToolRankingRow | undefined,
): TreeCuttingSection {
	const markupByItem = new Map<string, MarketToolItemMarkup>(
		(market?.items ?? []).map((item) => [item.itemName, item]),
	);
	const totalTt = tool.lootItems.reduce((sum, item) => sum + item.valuePed, 0);
	const items: TreeCuttingItem[] = tool.lootItems.map((item) => {
		const markup = markupByItem.get(item.itemName);
		return {
			name: item.itemName,
			quantity: item.quantity,
			ttValue: item.valuePed,
			sharePct: totalTt > 0 ? (item.valuePed / totalTt) * 100 : 0,
			markupPct: markup?.markupPct ?? null,
			markupHorizon: markup?.horizon ?? null,
		};
	});

	const muProjectedReturns = market?.muProjectedReturns ?? null;
	const muRate =
		muProjectedReturns !== null && tool.cycled > 0 ? muProjectedReturns / tool.cycled : null;
	const coverage =
		market && market.lootTt > 0 ? market.coveredTt / market.lootTt : market ? 0 : null;

	return {
		toolName: tool.toolName,
		tree: primaryTree(tool.lootItems),
		swings: tool.swings,
		cycled: tool.cycled,
		returns: tool.returns,
		lootRate: tool.lootRate,
		muProjectedReturns,
		muRate,
		coverage,
		items,
	};
}

export function createTreeCuttingModel() {
	let data = $state<HarvestData | null>(null);
	let market = $state<MarketToolRankingRow[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	async function loadData() {
		loading = true;
		error = null;
		try {
			// The realised aggregate is the spine; the market feed is
			// best-effort context, so a market failure degrades to the
			// realised view rather than blanking the tab.
			const [harvest, ranking] = await Promise.all([
				getAnalyticsHarvest(),
				getMarketToolRanking().catch(() => [] as MarketToolRankingRow[]),
			]);
			data = harvest;
			market = ranking;
		} catch (e) {
			error = describeError(e, 'Failed to load tree cutting data');
		} finally {
			loading = false;
		}
	}

	// Backend order preserved (swings-desc, cycled-desc, name).
	const sections = $derived.by<TreeCuttingSection[]>(() => {
		if (!data) return [];
		const marketByTool = new Map(market.map((row) => [row.toolName, row]));
		return data.toolComparisons.map((tool) => toSection(tool, marketByTool.get(tool.toolName)));
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
		loadData,
	};
}

export type TreeCuttingModel = ReturnType<typeof createTreeCuttingModel>;
