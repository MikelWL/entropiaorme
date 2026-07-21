/**
 * Tree Cutting-tab view model: the per-tool comparison data and its
 * sorted projection. Presentation lives in the tab component; it
 * composes over this state.
 */

import { getAnalyticsHarvest, type HarvestData } from '$lib/api';
import type { HarvestToolComparison } from '$lib/types/analytics';
import { describeError } from '$lib/view/errorState';

export type SortDir = 'asc' | 'desc';

/** Placeholder column key: markup rate arrives with the market-data feed. */
export const MU_RATE_KEY = '__muRate';

export const toolColumns = [
	{ key: 'toolName', label: 'Tool', sortable: true, widthClass: 'w-[32%]' },
	{
		key: 'swings',
		label: 'Swings',
		align: 'right' as const,
		sortable: true,
		widthClass: 'w-[17%]',
	},
	{
		key: 'cycled',
		label: 'Cycled',
		align: 'right' as const,
		sortable: true,
		widthClass: 'w-[17%]',
	},
	{
		key: 'lootRate',
		label: 'Rate',
		align: 'right' as const,
		sortable: true,
		widthClass: 'w-[17%]',
	},
	{
		key: MU_RATE_KEY,
		label: 'MU Rate',
		align: 'right' as const,
		sortable: false,
		widthClass: 'w-[17%]',
	},
];

function sortComparisons<T>(rows: T[], key: keyof T & string, dir: SortDir): T[] {
	return [...rows].sort((a, b) => {
		const aVal = a[key];
		const bVal = b[key];
		if (typeof aVal === 'number' && typeof bVal === 'number') {
			return dir === 'asc' ? aVal - bVal : bVal - aVal;
		}
		return dir === 'asc'
			? String(aVal).localeCompare(String(bVal))
			: String(bVal).localeCompare(String(aVal));
	});
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

	let toolSortKey = $state<(keyof HarvestToolComparison & string) | undefined>('cycled');
	let toolSortDir = $state<SortDir>('desc');

	const sortedTools = $derived.by(() => {
		if (!data) return [];
		if (!toolSortKey) return data.toolComparisons;
		return sortComparisons(data.toolComparisons, toolSortKey, toolSortDir);
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

		get toolSortKey() {
			return toolSortKey;
		},
		set toolSortKey(value: (keyof HarvestToolComparison & string) | undefined) {
			toolSortKey = value;
		},
		get toolSortDir() {
			return toolSortDir;
		},
		set toolSortDir(value: SortDir) {
			toolSortDir = value;
		},
		get sortedTools() {
			return sortedTools;
		},

		loadData,
	};
}

export type TreeCuttingModel = ReturnType<typeof createTreeCuttingModel>;
