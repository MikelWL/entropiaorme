/**
 * History-tab view model: one item's markup observations over time on
 * one aggregation horizon, oldest first. The item list comes from the
 * overview read (every item with at least one observation).
 */

import type { MarketHistoryPoint, MarketHorizon } from '$lib/api';
import { getMarketItemHistory, getMarketOverview } from '$lib/api';
import { describeError } from '$lib/view/errorState';

export function createHistoryModel() {
	let itemNames = $state<string[]>([]);
	let selectedItem = $state<string | null>(null);
	let horizon = $state<MarketHorizon>('week');
	let points = $state<MarketHistoryPoint[]>([]);
	let loading = $state(false);
	let error = $state<string | null>(null);

	let loadEpoch = 0;

	/** Loads the selectable item list; keeps the current selection when
	 * it still exists, else selects the first item. */
	async function loadItems() {
		error = null;
		try {
			const rows = await getMarketOverview();
			itemNames = rows.map((row) => row.itemName);
			if (selectedItem === null || !itemNames.includes(selectedItem)) {
				selectedItem = itemNames[0] ?? null;
			}
			await loadPoints();
		} catch (e) {
			error = describeError(e, 'Failed to load the item list');
		}
	}

	async function loadPoints() {
		const item = selectedItem;
		const epoch = ++loadEpoch;
		if (item === null) {
			points = [];
			return;
		}
		loading = true;
		error = null;
		try {
			const loaded = await getMarketItemHistory(item, horizon);
			if (epoch !== loadEpoch) return;
			points = loaded;
		} catch (e) {
			if (epoch !== loadEpoch) return;
			error = describeError(e, 'Failed to load the item history');
		} finally {
			if (epoch === loadEpoch) loading = false;
		}
	}

	function selectItem(item: string) {
		selectedItem = item;
		void loadPoints();
	}

	function selectHorizon(value: MarketHorizon) {
		horizon = value;
		void loadPoints();
	}

	return {
		get itemNames() {
			return itemNames;
		},
		get selectedItem() {
			return selectedItem;
		},
		get horizon() {
			return horizon;
		},
		get points() {
			return points;
		},
		get loading() {
			return loading;
		},
		get error() {
			return error;
		},
		loadItems,
		selectItem,
		selectHorizon,
	};
}

export type HistoryModel = ReturnType<typeof createHistoryModel>;
