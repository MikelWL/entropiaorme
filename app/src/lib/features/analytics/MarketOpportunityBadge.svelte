<script lang="ts">
	import InfoTip from '$lib/components/InfoTip.svelte';
	import type { MarketOpportunity, OpportunityKind } from './treeCuttingModel.svelte';
	import { formatPed, formatPercent } from '$lib/utils/format';

	let {
		opportunity,
		align = 'right',
	}: {
		opportunity: MarketOpportunity;
		align?: 'left' | 'right';
	} = $props();

	const LABELS: Record<OpportunityKind, string> = {
		broad: 'Broad',
		niche: 'Niche',
		thin: 'Thin',
		recycle: 'Recycle',
	};

	const CLASSES: Record<OpportunityKind, string> = {
		broad: 'border-positive/35 bg-positive/10 text-positive',
		niche: 'border-accent/35 bg-accent/10 text-accent',
		thin: 'border-warning/35 bg-warning/10 text-warning',
		recycle: 'border-border/60 bg-surface-hover/50 text-text-secondary',
	};

	function formatVolume(value: number): string {
		if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M PED`;
		if (value >= 1_000) return `${(value / 1_000).toFixed(1)}K PED`;
		return `${formatPed(value)} PED`;
	}

	function lead(value: MarketOpportunity): string {
		if (value.kind === 'broad') return 'Broad market: an efficient parcel is small beside weekly turnover.';
		if (value.kind === 'niche') return 'Niche market: high unit margin offsets sparse trading cadence.';
		if (value.kind === 'thin') return 'Thin market: a direct premium exists, but capacity is constrained.';
		if (value.ownMarkupPct == null) return 'No supported direct market observation. Valued through Nanocubes.';
		if (value.ownMarkupPct < value.appliedMarkupPct) {
			return 'The Nanocube route currently pays more than this item’s direct market.';
		}
		return 'The observed direct market does not support a fee-efficient parcel. Valued through Nanocubes.';
	}
</script>

<InfoTip {align} width="w-80" label={`${LABELS[opportunity.kind]} market opportunity`}>
	{#snippet trigger()}
		<span
			class="inline-flex rounded-full border px-2 py-0.5 text-[10px] font-semibold uppercase tracking-[0.12em]
				{CLASSES[opportunity.kind]}"
		>
			{LABELS[opportunity.kind]}
		</span>
	{/snippet}

	<div class="space-y-2 text-xs leading-relaxed text-text-secondary">
		<p class="text-text">{lead(opportunity)}</p>

		{#if opportunity.ownMarkupPct !== null}
			<div class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1">
				<span>Direct MU</span>
				<span class="text-right tabular-nums text-text">
					{formatPercent(opportunity.ownMarkupPct / 100)}
				</span>
				{#if opportunity.salesPed !== null && opportunity.horizon}
					<span>Observed turnover</span>
					<span class="text-right tabular-nums text-text">
						{formatVolume(opportunity.salesPed)} / {opportunity.horizon}
					</span>
				{/if}
				{#if opportunity.efficientBatchTt !== null}
					<span>Fee-efficient parcel</span>
					<span class="text-right tabular-nums text-text">
						{formatPed(opportunity.efficientBatchTt)} PED TT
					</span>
				{/if}
				{#if opportunity.efficientBatchMarketShare !== null}
					<span>Observed-horizon share</span>
					<span class="text-right tabular-nums text-text">
						{formatPercent(opportunity.efficientBatchMarketShare)}
					</span>
				{/if}
				{#if opportunity.efficientBatchMarketWeeks !== null}
					<span>Turnover equivalent</span>
					<span class="text-right tabular-nums text-text">
						{opportunity.efficientBatchMarketWeeks.toFixed(2)} weeks
					</span>
				{/if}
				{#if opportunity.weeklyPremiumThroughput > 0}
					<span>Premium throughput</span>
					<span class="text-right tabular-nums text-text">
						{formatVolume(opportunity.weeklyPremiumThroughput)} / week
					</span>
				{/if}
			</div>
		{/if}

		{#if opportunity.usesNanocube}
			<p>
				Activity valuation uses the Nanocube market floor at
				<span class="tabular-nums text-text">
					{formatPercent(opportunity.appliedMarkupPct / 100)}
				</span>.
			</p>
		{/if}

		<p class="text-text-tertiary">
			Market-wide evidence only. This classification does not use your current stock or promise
			a sale time.
		</p>
	</div>
</InfoTip>
