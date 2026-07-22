<script lang="ts">
	import Card from '$lib/components/Card.svelte';
	import InfoTip from '$lib/components/InfoTip.svelte';
	import type { TreeCuttingItem, TreeCuttingSection } from './treeCuttingModel.svelte';
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

	const NO_DATA = String.fromCharCode(8212);
	const signedPed = (value: number) => `${value >= 0 ? '+' : ''}${formatPed(value)}`;
	const netTone = (value: number) => (value >= 0 ? 'text-positive' : 'text-negative');

	function confidenceTip(item: TreeCuttingItem): { lead: string; detail?: string } {
		const floorNote = item.floored ? ' Valued at the nanocube rate instead.' : '';
		if (item.ownMarkupPct == null) {
			return { lead: 'No market data for this item.', detail: floorNote.trim() || undefined };
		}
		if (item.opportunity.kind === 'broad') {
			return {
				lead: `High volume: ~${formatPed(item.salesPed ?? 0)} PED TT traded last week.`,
				detail: 'The fee-efficient parcel is small beside observed weekly turnover.',
			};
		}
		if (item.opportunity.kind === 'niche') {
			return {
				lead: 'Medium confidence: sparse trading, but strong unit markup.',
				detail: `The direct market can amortise fees despite its cadence.${floorNote}`,
			};
		}
		if (item.opportunity.kind === 'thin' && item.markupHorizon === 'week') {
			return {
				lead: `Medium confidence: ~${formatPed(item.salesPed ?? 0)} PED TT traded last week.`,
				detail: 'A fee-efficient parcel fits observed turnover, but would take a meaningful share of it.',
			};
		}
		if (item.markupHorizon && item.markupHorizon !== 'week') {
			const weekly = item.weeklySalesPed;
			const lead =
				weekly == null || weekly <= 0
					? 'No sales in the last week.'
					: `Only ${formatPed(weekly)} PED TT traded last week.`;
			const range =
				item.salesPed != null
					? `Priced from the last ${item.markupHorizon} (${formatPed(item.salesPed)} PED TT traded).`
					: '';
			return { lead, detail: `${range}${floorNote}` };
		}
		return {
			lead: `Low volume: ~${formatPed(item.salesPed ?? 0)} PED TT traded last week.`,
			detail: `The direct premium has constrained fee-efficient capacity.${floorNote}`,
		};
	}
</script>

{#snippet confidenceBody(item: TreeCuttingItem)}
	{@const tip = confidenceTip(item)}
	<p class="text-xs leading-relaxed text-text">{tip.lead}</p>
	{#if tip.detail}
		<p class="mt-1 text-xs leading-relaxed text-text-secondary">{tip.detail}</p>
	{/if}
{/snippet}

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
			<span class="w-16 shrink-0 text-right text-xs tabular-nums text-text">
				{formatPed(section.cycled)}
			</span>
			<span
				class="w-16 shrink-0 text-right text-xs tabular-nums {netTone(section.returns - section.cycled)}"
			>
				{signedPed(section.returns - section.cycled)}
			</span>
			<span class="w-16 shrink-0 text-right text-xs tabular-nums text-text">
				{section.muProjectedReturns !== null
					? signedPed(section.muProjectedReturns - section.cycled)
					: NO_DATA}
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
					<span class="eyebrow w-16 shrink-0 text-right">Cycled</span>
					<span class="eyebrow w-16 shrink-0 text-right">TT Net</span>
					<span class="eyebrow w-16 shrink-0 text-right">MU Net</span>
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
					returns={selected.returns}
					lootRate={selected.lootRate}
					muProjectedReturns={selected.muProjectedReturns}
					muRate={selected.muRate}
					realisedReturns={selected.realisedReturns}
					realisedRate={selected.realisedRate}
				/>

				{#if selected.items.length > 0}
					<div class="mt-5 border-t border-border/50 pt-4">
							<div class="flex items-center gap-3 px-2.5 pb-1 text-text-tertiary">
								<span class="eyebrow flex-1 min-w-0">Item</span>
								<span class="eyebrow w-20 text-right shrink-0">TT</span>
								<span class="eyebrow w-14 text-right shrink-0">Share</span>
								<span class="eyebrow w-36 text-right shrink-0">Markup</span>
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
										class="text-sm tabular-nums shrink-0 w-36 text-right flex items-center justify-end gap-1.5"
									>
										{#if selected.muProjectedReturns === null}
											<span class="text-text-tertiary">{NO_DATA}</span>
										{:else}
											{#if item.tier === 'middling'}
												<InfoTip align="right" label="Medium volume">
													{#snippet trigger()}
														<span class="text-warning">⚠</span>
													{/snippet}
													{@render confidenceBody(item)}
												</InfoTip>
											{:else if item.tier === 'illiquid'}
												<InfoTip align="right" label="Low volume">
													{#snippet trigger()}
														<span class="text-error font-semibold">!</span>
													{/snippet}
													{@render confidenceBody(item)}
												</InfoTip>
											{/if}
											{#if item.floored && item.ownMarkupPct !== null}
												<span class="text-text-tertiary line-through">
													{formatPercent(item.ownMarkupPct / 100)}
												</span>
												<span class="text-text-secondary">
													{formatPercent(item.effectiveMarkupPct / 100)}
												</span>
											{:else}
												<span class="text-text-secondary">
													{formatPercent(item.effectiveMarkupPct / 100)}
												</span>
											{/if}
										{/if}
									</span>
									</li>
								{/each}
							</ul>
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
