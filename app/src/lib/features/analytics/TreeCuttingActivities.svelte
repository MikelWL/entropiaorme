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
	const marketPeriod = (horizon: string) =>
		horizon === 'week' || horizon === 'month' || horizon === 'year'
			? `last ${horizon}`
			: `over the last ${horizon}`;
	const marketShare = (value: number) => {
		const percent = value * 100;
		return percent < 0.1 ? 'less than 0.1%' : `${percent.toFixed(1)}%`;
	};
	const confidenceTitle = (tier: TreeCuttingItem['tier']) => {
		if (tier === 'liquid') return 'High markup confidence: This markup should be practical to realise';
		if (tier === 'middling') {
			return 'Medium markup confidence: It may be difficult to realise this markup';
		}
		return 'Low markup confidence: Do not rely on realising this markup';
	};
	const markupLabel = (item: TreeCuttingItem) => {
		if (item.floored && item.ownMarkupPct !== null) {
			return `Observed markup ${formatPercent(item.ownMarkupPct / 100)}; projections use ${formatPercent(item.effectiveMarkupPct / 100)} Nanocube markup`;
		}
		if (item.ownMarkupPct == null) {
			return `Projections use ${formatPercent(item.effectiveMarkupPct / 100)} Nanocube markup`;
		}
		return `Markup ${formatPercent(item.effectiveMarkupPct / 100)}`;
	};

	function confidenceTip(item: TreeCuttingItem): {
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
		const batchMarkup =
			batchTt === null ? null : batchTt * Math.max(0, item.ownMarkupPct / 100 - 1);
		const example =
			batchTt !== null && batchMarkup !== null && batchShare !== null && horizon
				? `For example: A ${formatPed(batchTt)} PED TT sale at this MU would produce about ${formatPed(batchMarkup)} PED of markup. The minimum auction fee is 0.5 PED, or 10% of that markup. That sale would be ${marketShare(batchShare)} of the TT sold ${marketPeriod(horizon)}.`
				: undefined;
		const noExampleNote = example
			? projectionNote
			: ['The recorded MU does not provide enough markup to calculate a sale after fees.', projectionNote]
					.filter(Boolean)
					.join(' ');
		return {
			title: confidenceTitle(item.tier),
			subtitle: lead,
			example,
			note: noExampleNote || undefined,
		};
	}
</script>

{#snippet confidenceBody(item: TreeCuttingItem)}
	{@const tip = confidenceTip(item)}
	<p class="text-xs font-semibold leading-relaxed text-text">{tip.title}</p>
	<p class="mt-1 text-xs leading-relaxed text-text-secondary">{tip.subtitle}</p>
	{#if tip.example}
		<p class="mt-2 text-xs leading-relaxed text-text-secondary">{tip.example}</p>
	{/if}
	{#if tip.note}
		<p class="mt-2 text-xs leading-relaxed text-text-tertiary">{tip.note}</p>
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
								<span class="eyebrow w-20 text-right shrink-0">Markup</span>
								<span class="eyebrow w-12 text-center shrink-0">Conf</span>
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

									<div class="w-20 shrink-0 flex items-center justify-end">
										{#if selected.muProjectedReturns === null}
											<span class="text-sm text-text-tertiary">{NO_DATA}</span>
										{:else}
											<span
												class="inline-flex h-5 flex-col items-end justify-center tabular-nums"
												aria-label={markupLabel(item)}
											>
												{#if item.floored && item.ownMarkupPct !== null}
													{@const observedMarkup = item.ownMarkupPct}
													<span class="text-[9px] leading-[9px] text-text-tertiary line-through">
														{formatPercent(observedMarkup / 100)}
													</span>
													<span class="text-xs leading-[11px] text-text-secondary">
														{formatPercent(item.effectiveMarkupPct / 100)}
													</span>
												{:else}
													<span class="text-sm leading-5 text-text-secondary">
														{formatPercent(item.effectiveMarkupPct / 100)}
													</span>
												{/if}
											</span>
										{/if}
									</div>

									<div class="w-12 shrink-0 flex items-center justify-center">
										{#if selected.muProjectedReturns === null}
											<span class="text-sm text-text-tertiary">{NO_DATA}</span>
										{:else}
											<InfoTip
												align="right"
												width="w-96"
												label={confidenceTitle(item.tier)}
											>
												{#snippet trigger()}
													{#if item.tier === 'liquid'}
														<span class="text-positive" aria-label="High volume">✓</span>
													{:else if item.tier === 'middling'}
														<span class="text-warning" aria-label="Medium volume">⚠</span>
													{:else}
														<span class="text-error font-semibold" aria-label="Low volume">!</span>
													{/if}
												{/snippet}
												{@render confidenceBody(item)}
											</InfoTip>
										{/if}
									</div>
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
