/**
 * The market-confidence copy shared by the activity item-composition
 * tables (Tree Cutting sub-activities, Hunting sessions and mobs): one source for
 * the confidence titles, the applied-markup labels, and the evidence
 * tooltip, so the two tabs explain the same model in the same words.
 */

import { formatPed, formatPercent } from '$lib/utils/format';
import type { TreeCuttingItem } from './treeCuttingModel.svelte';

export const marketPeriod = (horizon: string) =>
	horizon === 'week' || horizon === 'month' || horizon === 'year'
		? `last ${horizon}`
		: `over the last ${horizon}`;

export const marketShare = (value: number) => {
	const percent = value * 100;
	return percent < 0.1 ? 'less than 0.1%' : `${percent.toFixed(1)}%`;
};

export const confidenceTitle = (tier: TreeCuttingItem['tier']) => {
	if (tier === 'liquid')
		return 'High markup confidence: This markup should be practical to realise';
	if (tier === 'middling') {
		return 'Medium markup confidence: It may be difficult to realise this markup';
	}
	return 'Low markup confidence: Do not rely on realising this markup';
};

export const markupLabel = (item: TreeCuttingItem) => {
	if (item.floored && item.ownMarkupPct !== null) {
		return `Observed markup ${formatPercent(item.ownMarkupPct / 100)}; projections use ${formatPercent(item.effectiveMarkupPct / 100)} Nanocube markup`;
	}
	if (item.ownMarkupPct == null) {
		return `Projections use ${formatPercent(item.effectiveMarkupPct / 100)} Nanocube markup`;
	}
	return `Markup ${formatPercent(item.effectiveMarkupPct / 100)}`;
};

/** The evidence tooltip for one composition item: what sold, at what
 * markup, over which horizon, and the fee-efficient worked example. */
export function confidenceTip(item: TreeCuttingItem): {
	title: string;
	subtitle: string;
	example?: string;
	note?: string;
} {
	const projectionNote = item.floored
		? `With the current confidence setting, MU projections use the ${formatPercent(item.effectiveMarkupPct / 100)} Nanocube MU instead.`
		: undefined;
	if (item.ownMarkupPct == null) {
		return {
			title: confidenceTitle(item.tier),
			subtitle: 'No market MU is available for this item.',
			note: projectionNote,
		};
	}

	const horizon = item.markupHorizon;
	const salesPed = item.salesPed;
	let lead = 'No recent sales data is available for this item.';
	if (horizon && salesPed !== null) {
		if (horizon === 'week') {
			lead = `${formatPed(salesPed)} PED TT sold last week at ${formatPercent(item.ownMarkupPct / 100)} MU.`;
		} else {
			const weekly = item.weeklySalesPed;
			const weeklyReading =
				weekly == null || weekly <= 0
					? 'No sales in the last week.'
					: `${formatPed(weekly)} PED TT sold last week.`;
			lead = `${weeklyReading} The current ${formatPercent(item.ownMarkupPct / 100)} MU comes from ${formatPed(salesPed)} PED TT sold ${marketPeriod(horizon)}.`;
		}
	}

	const batchTt = item.opportunity.efficientBatchTt;
	const batchShare = item.opportunity.efficientBatchMarketShare;
	const batchMarkup = batchTt === null ? null : batchTt * Math.max(0, item.ownMarkupPct / 100 - 1);
	const example =
		batchTt !== null && batchMarkup !== null && batchShare !== null && horizon
			? `For example: A ${formatPed(batchTt)} PED TT sale at this MU would produce about ${formatPed(batchMarkup)} PED of markup. The minimum auction fee is 0.5 PED, or 10% of that markup. That sale would be ${marketShare(batchShare)} of the TT sold ${marketPeriod(horizon)}.`
			: undefined;
	const insufficientMarkup = batchTt === null || batchMarkup === null;
	const noExampleNote = example
		? projectionNote
		: [
				insufficientMarkup
					? 'The recorded MU does not provide enough markup to calculate a sale after fees.'
					: undefined,
				projectionNote,
			]
				.filter(Boolean)
				.join(' ');
	return {
		title: confidenceTitle(item.tier),
		subtitle: lead,
		example,
		note: noExampleNote || undefined,
	};
}
