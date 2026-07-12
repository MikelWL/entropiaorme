/**
 * Mobs-tab view model: every hunted species' estimated loot markup on
 * a selected aggregation window, TT-weighted over the recorded loot
 * composition, with coverage as the honesty signal. The backend
 * computes and orders the rows; this model loads them and derives the
 * presentation figures.
 */

import type { MarketHorizon, MarketMobRankingRow } from '$lib/api';
import { getMarketMobRanking } from '$lib/api';
import { describeError } from '$lib/view/errorState';

/** Coverage as a whole percent of the species' loot TT (0 when none). */
export function coveragePct(row: MarketMobRankingRow): number {
	return row.lootTt > 0 ? Math.round((row.coveredTt / row.lootTt) * 100) : 0;
}

export function createMobsModel() {
	let rows = $state<MarketMobRankingRow[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let horizon = $state<MarketHorizon>('week');

	let loadEpoch = 0;

	async function loadData() {
		const epoch = ++loadEpoch;
		loading = true;
		error = null;
		try {
			const loaded = await getMarketMobRanking(horizon);
			if (epoch !== loadEpoch) return;
			rows = loaded;
		} catch (e) {
			if (epoch !== loadEpoch) return;
			error = describeError(e, 'Failed to load the mob ranking');
		} finally {
			if (epoch === loadEpoch) loading = false;
		}
	}

	function selectHorizon(value: MarketHorizon) {
		horizon = value;
		void loadData();
	}

	return {
		get rows() {
			return rows;
		},
		get loading() {
			return loading;
		},
		get error() {
			return error;
		},
		get horizon() {
			return horizon;
		},
		loadData,
		selectHorizon,
	};
}

export type MobsModel = ReturnType<typeof createMobsModel>;
