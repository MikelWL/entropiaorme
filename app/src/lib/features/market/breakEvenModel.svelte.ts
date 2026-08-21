/**
 * Break-even-tab view model over configured loadouts. Component weighting,
 * limited-item premium drag, and the exact-three looter fallback are all
 * computed backend-side; this model only orders the readout.
 */

import type { MarketBreakEven } from '$lib/api';
import { getMarketBreakEven } from '$lib/api';
import { describeError } from '$lib/view/errorState';

export function createBreakEvenModel() {
	let data = $state<MarketBreakEven | null>(null);
	let loading = $state(true);
	let error = $state<string | null>(null);

	let loadEpoch = 0;

	async function loadData() {
		const epoch = ++loadEpoch;
		loading = true;
		error = null;
		try {
			const loaded = await getMarketBreakEven();
			if (epoch !== loadEpoch) return;
			data = loaded;
		} catch (e) {
			if (epoch !== loadEpoch) return;
			error = describeError(e, 'Failed to load the break-even readout');
		} finally {
			if (epoch === loadEpoch) loading = false;
		}
	}

	const looters = $derived(data?.looters ?? []);

	// Complete loadouts first, closest break-even requirement first. Missing
	// Efficiency remains visible at the end rather than acquiring a zero.
	const weapons = $derived.by(() => {
		const rows = [...(data?.weapons ?? [])];
		return rows.sort(
			(a, b) => (a.breakEvenLootMarkupPct ?? Infinity) - (b.breakEvenLootMarkupPct ?? Infinity),
		);
	});

	return {
		get looters() {
			return looters;
		},
		get weapons() {
			return weapons;
		},
		get loading() {
			return loading;
		},
		get error() {
			return error;
		},
		loadData,
	};
}

export type BreakEvenModel = ReturnType<typeof createBreakEvenModel>;
