/**
 * Break-even-tab view model: every library weapon's modelled break-even
 * markup against each of the player's looter professions. The figures
 * are modelled estimates (community returns model, roughly a one
 * percentage point error bar), computed backend-side; this model only
 * loads and shapes them for the table.
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

	// Weapons the catalogue knows first (they carry cells), unknown-
	// efficiency weapons last, each group by ascending best break-even.
	const weapons = $derived.by(() => {
		const rows = [...(data?.weapons ?? [])];
		const best = (cells: { breakEvenMarkupPct: number }[]) =>
			cells.length ? Math.min(...cells.map((c) => c.breakEvenMarkupPct)) : Infinity;
		return rows.sort((a, b) => best(a.cells) - best(b.cells));
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
