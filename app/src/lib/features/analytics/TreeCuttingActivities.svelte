<script lang="ts">
	import Card from '$lib/components/Card.svelte';
	import type { TreeCuttingSection } from './treeCuttingModel.svelte';
	import MarketOpportunityBadge from './MarketOpportunityBadge.svelte';
	import TreeCuttingStats from './TreeCuttingStats.svelte';
	import { formatPed, formatPercent } from '$lib/utils/format';

	let {
		sections,
		selected,
		onselect,
	}: {
		sections: TreeCuttingSection[];
		selected: TreeCuttingSection | null;
		onselect: (toolName: string) => void;
	} = $props();

	const NO_DATA = 'N/A';
	const signedPed = (value: number) => `${value >= 0 ? '+' : ''}${formatPed(value)}`;
	const netTone = (value: number) => (value >= 0 ? 'text-positive' : 'text-negative');
</script>

{#snippet subActivityRow(section: TreeCuttingSection, isSelected: boolean)}
	<li>
		<button
			type="button"
			aria-pressed={isSelected}
			onclick={() => onselect(section.toolName)}
			class="w-full flex items-center gap-2.5 rounded-lg border px-3 py-2 text-left
				transition-[background-color,border-color] duration-[var(--duration-base)] ease-[var(--ease-out)]
				{isSelected
					? 'border-accent/40 bg-accent/[0.08]'
					: 'border-transparent hover:border-border/40 hover:bg-surface-hover/40'}"
		>
			<span class="flex-1 min-w-0 truncate text-sm font-medium tracking-tight text-text">
				{section.tree ? `${section.tree} Trees` : section.toolName}
			</span>
			<span
				class="w-16 shrink-0 text-right text-xs tabular-nums {netTone(section.returns - section.cycled)}"
			>
				{signedPed(section.returns - section.cycled)}
			</span>
			<span class="w-16 shrink-0 text-right text-xs tabular-nums text-text">
				{section.marketReturns !== null
					? signedPed(section.marketReturns - section.cycled)
					: NO_DATA}
			</span>
			<span
				class="w-16 shrink-0 text-right text-xs tabular-nums {netTone(
					section.realisedReturns - section.cycled,
				)}"
			>
				{signedPed(section.realisedReturns - section.cycled)}
			</span>
		</button>
	</li>
{/snippet}

<Card class="hover:z-20">
	<div class="grid sm:grid-cols-[minmax(0,21rem)_1fr]">
		<div class="border-b border-border/40 sm:border-b-0 sm:border-r">
			<div class="px-2 pt-4">
				<div
					class="flex items-center gap-2.5 rounded-lg border border-transparent px-3 pb-2 text-text-tertiary"
				>
					<span class="eyebrow flex-1 min-w-0">Activity</span>
					<span class="eyebrow w-16 shrink-0 text-right">TT</span>
					<span class="eyebrow w-16 shrink-0 text-right text-accent">Market</span>
					<span class="eyebrow w-16 shrink-0 text-right text-positive">Realised</span>
				</div>
			</div>
			<ul class="flex flex-col gap-1 px-2 pb-3 max-h-[26rem] overflow-y-auto">
				{#each sections as section (section.toolName)}
					{@render subActivityRow(section, section.toolName === selected?.toolName)}
				{/each}
			</ul>
		</div>

		{#if selected}
			<div class="p-5">
				<TreeCuttingStats
					cycled={selected.cycled}
					swings={selected.swings}
					returns={selected.returns}
					lootRate={selected.lootRate}
					marketReturns={selected.marketReturns}
					marketRate={selected.marketRate}
					realisedReturns={selected.realisedReturns}
					realisedRate={selected.realisedRate}
				/>

				{#if selected.items.length > 0}
					<div class="mt-5 border-t border-border/50 pt-4 overflow-x-auto">
						<div class="min-w-[31rem]">
							<div class="flex items-center gap-3 px-2.5 pb-1 text-text-tertiary">
								<span class="eyebrow flex-1 min-w-0">Item</span>
								<span class="eyebrow w-20 text-right shrink-0">TT</span>
								<span class="eyebrow w-14 text-right shrink-0">Share</span>
								<span class="eyebrow w-24 text-right shrink-0">Current MU</span>
								<span class="eyebrow w-16 text-center shrink-0">Market</span>
							</div>

							<ul class="flex flex-col gap-1">
								{#each selected.items as item (item.name)}
									<li
										class="flex items-center gap-3 rounded-md px-2.5 py-2 border border-transparent
											hover:bg-surface-hover/30 hover:border-border/40
											transition-[background-color,border-color] duration-[var(--duration-base)] ease-[var(--ease-out)]"
									>
										<div class="flex-1 min-w-0 flex items-baseline gap-2">
											<span class="text-sm font-medium truncate tracking-tight text-text">
												{item.name}
											</span>
											<span class="text-xs text-text-tertiary tabular-nums shrink-0">
												×{item.quantity}
											</span>
										</div>

										<span class="text-sm tabular-nums font-medium text-text shrink-0 w-20 text-right">
											{formatPed(item.ttValue)}
										</span>

										<span
											class="text-sm tabular-nums font-semibold text-accent shrink-0 w-14 text-right tracking-tight"
										>
											{item.sharePct.toFixed(1)}%
										</span>

										<span
											class="w-24 shrink-0 text-right text-sm tabular-nums text-text-secondary"
										>
											{#if item.opportunity.usesNanocube && item.opportunity.ownMarkupPct !== null}
												<span class="mr-1 text-text-tertiary line-through">
													{formatPercent(item.opportunity.ownMarkupPct / 100)}
												</span>
											{/if}
											{formatPercent(item.opportunity.appliedMarkupPct / 100)}
										</span>

										<span class="w-16 shrink-0 flex items-center justify-center">
											<MarketOpportunityBadge opportunity={item.opportunity} />
										</span>
									</li>
								{/each}
							</ul>
						</div>
					</div>
				{:else}
					<p class="mt-4 text-xs text-text-tertiary px-2.5">
						No loot recorded on this tool yet.
					</p>
				{/if}
			</div>
		{/if}
	</div>
</Card>
