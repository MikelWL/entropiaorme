/**
 * Tree Cutting-tab view model. Each harvesting tool the player has used
 * becomes its own section: a realised stat strip (swings, cycled,
 * returns, rate) over a per-item loot breakdown. The section is titled
 * by the primary tree the tool has been cutting, inferred from its
 * dominant board type.
 *
 * The market-derived columns (MU Projected Returns, MU Rate, and the
 * per-item markup) are placeholders here: they arrive from the market
 * layer and merge in a later slice, never joined into this accounting
 * read.
 */

import { getAnalyticsHarvest, type HarvestData } from '$lib/api';
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

function toSection(tool: HarvestToolComparison): TreeCuttingSection {
	const totalTt = tool.lootItems.reduce((sum, item) => sum + item.valuePed, 0);
	const items: TreeCuttingItem[] = tool.lootItems.map((item) => ({
		name: item.itemName,
		quantity: item.quantity,
		ttValue: item.valuePed,
		sharePct: totalTt > 0 ? (item.valuePed / totalTt) * 100 : 0,
	}));
	return {
		toolName: tool.toolName,
		tree: primaryTree(tool.lootItems),
		swings: tool.swings,
		cycled: tool.cycled,
		returns: tool.returns,
		lootRate: tool.lootRate,
		items,
	};
}

export function createTreeCuttingModel() {
	let data = $state<HarvestData | null>(null);
	let loading = $state(true);
	let error = $state<string | null>(null);

	async function loadData() {
		loading = true;
		error = null;
		try {
			data = await getAnalyticsHarvest();
		} catch (e) {
			error = describeError(e, 'Failed to load tree cutting data');
		} finally {
			loading = false;
		}
	}

	// Backend order preserved (swings-desc, cycled-desc, name).
	const sections = $derived.by<TreeCuttingSection[]>(() =>
		data ? data.toolComparisons.map(toSection) : [],
	);

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
