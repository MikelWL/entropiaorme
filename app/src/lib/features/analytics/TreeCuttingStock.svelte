<script lang="ts">
	import InfoTip from '$lib/components/InfoTip.svelte';
	import type { TreeCuttingStock } from './treeCuttingModel.svelte';
	import MarketOpportunityBadge from './MarketOpportunityBadge.svelte';
	import { formatPed, formatPercent } from '$lib/utils/format';

	let { stock }: { stock: TreeCuttingStock[] } = $props();

	const NO_DATA = 'N/A';

	function formatVolume(value: number): string {
		if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M`;
		if (value >= 1_000) return `${(value / 1_000).toFixed(1)}K`;
		return formatPed(value);
	}
</script>

{#snippet actionButton(letter: string, label: string, expandedWidth: string)}
	<button
		type="button"
		aria-label={label}
		class="group/act relative inline-flex h-6 w-6 shrink-0 items-center justify-center overflow-hidden
			rounded-md border border-border/60 bg-surface-hover/40 text-xs font-semibold text-text-secondary
			transition-[width,background-color,color,border-color] duration-[var(--duration-base)] ease-[var(--ease-out)]
			{expandedWidth} hover:text-text hover:border-border hover:bg-surface-hover/70"
	>
		<span
			class="absolute inset-0 flex items-center justify-center
				transition-opacity duration-[var(--duration-fast)] group-hover/act:opacity-0"
		>
			{letter}
		</span>
		<span
			class="absolute inset-0 flex items-center justify-center whitespace-nowrap px-2
				opacity-0 transition-opacity duration-[var(--duration-fast)] group-hover/act:opacity-100"
		>
			{label}
		</span>
	</button>
{/snippet}

{#snippet markupBreakdown(
	readings: { horizon: string; markupPct: number | null; salesPed: number }[],
)}
	<div class="grid grid-cols-[auto_repeat(4,minmax(2.25rem,1fr))] items-center gap-x-3 gap-y-1.5">
		<span></span>
		{#each readings as reading (reading.horizon)}
			<span class="eyebrow text-right">{reading.horizon}</span>
		{/each}

		<span class="eyebrow">MU</span>
		{#each readings as reading (reading.horizon)}
			<span class="text-right text-sm tabular-nums text-text">
				{reading.markupPct !== null ? formatPercent(reading.markupPct / 100) : NO_DATA}
			</span>
		{/each}

		<span class="eyebrow">Volume</span>
		{#each readings as reading (reading.horizon)}
			<span class="text-right text-sm tabular-nums text-text-secondary">
				{reading.salesPed > 0 ? formatVolume(reading.salesPed) : NO_DATA}
			</span>
		{/each}
	</div>
{/snippet}

<div class="sm:border-l sm:border-border/40 sm:pl-8">
	<div class="flex items-center gap-2 pb-2">
		<span class="eyebrow">Current stock</span>
		<InfoTip align="right" label="What current stock means">
			<div class="space-y-2 text-xs leading-relaxed text-text-secondary">
				<p class="text-text">
					How much of each item remains from everything recorded through Tree Cutting.
				</p>
				<p>
					Stock supports sale and recycling decisions. It does not change the Current Market
					figures, which assess the market independently of what you happen to hold.
				</p>
				<p>
					For now this shows recorded harvest. Confirmed transactions will later keep the position
					and realised results in sync automatically.
				</p>
			</div>
		</InfoTip>
	</div>

	<div class="overflow-x-auto">
		<div class="min-w-[31rem]">
			<div class="flex items-center gap-3 px-2.5 pb-1 text-text-tertiary">
				<span class="eyebrow flex-1 min-w-0">Item</span>
				<span class="eyebrow w-20 text-right shrink-0">Stock TT</span>
				<span class="eyebrow w-20 text-right shrink-0">Current MU</span>
				<span class="eyebrow w-16 text-center shrink-0">Market</span>
				<span class="w-[3.375rem] shrink-0"></span>
			</div>

			<ul class="flex flex-col gap-1">
				{#each stock as item (item.itemName)}
					<li class="flex items-center gap-3 rounded-md px-2.5 py-2">
						<span class="flex-1 min-w-0 text-sm font-medium truncate tracking-tight text-text">
							{item.itemName}
						</span>

						<span class="w-20 text-right shrink-0 text-sm tabular-nums font-medium text-text">
							{formatPed(item.heldTt)}
						</span>

						<div class="w-20 shrink-0 flex items-center justify-end">
							{#if item.opportunity}
								{@const opportunity = item.opportunity}
								<InfoTip align="right" width="w-96" label="Markup by horizon">
									{#snippet trigger()}
										<span
											class="text-sm tabular-nums text-text-secondary border-b border-dotted border-border/70"
										>
											{formatPercent(opportunity.appliedMarkupPct / 100)}
										</span>
									{/snippet}
									{@render markupBreakdown(item.readings)}
								</InfoTip>
							{:else}
								<span class="text-sm text-text-tertiary">{NO_DATA}</span>
							{/if}
						</div>

						<div class="w-16 shrink-0 flex items-center justify-center">
							{#if item.opportunity}
								<MarketOpportunityBadge opportunity={item.opportunity} />
							{:else}
								<span class="text-sm text-text-tertiary">{NO_DATA}</span>
							{/if}
						</div>

						<div class="shrink-0 flex items-center justify-end gap-1.5">
							{@render actionButton('N', 'Turn into Nanocube', 'hover:w-44')}
							{@render actionButton('S', 'Sell', 'hover:w-16')}
						</div>
					</li>
				{/each}
			</ul>
		</div>
	</div>
</div>
