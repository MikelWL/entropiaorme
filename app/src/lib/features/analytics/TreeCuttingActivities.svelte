<script lang="ts">
	import Card from '$lib/components/Card.svelte';
	import InfoTip from '$lib/components/InfoTip.svelte';
	import StatDisplay from '$lib/components/StatDisplay.svelte';
	import type { SortDir, SortKey } from '$lib/view/tableModel.svelte';
	import {
		treeCuttingActivityName,
		type TreeCuttingActivitySortKey,
		type TreeCuttingItem,
		type TreeCuttingSection,
	} from './treeCuttingModel.svelte';
	import { NO_DATA, formatPed, formatPercent } from '$lib/utils/format';

	let {
		sections,
		selected,
		onselect,
		sortKey,
		sortDir,
		onsort,
	}: {
		sections: TreeCuttingSection[];
		selected: TreeCuttingSection | null;
		onselect: (yieldTier: TreeCuttingSection['yieldTier']) => void;
		sortKey: SortKey<TreeCuttingSection> | undefined;
		sortDir: SortDir;
		onsort: (key: TreeCuttingActivitySortKey) => void;
	} = $props();

	// Unclassified is pinned after the classified activities whatever the sort.
	// This re-partitions downstream of the sort the parent applied, deliberately:
	// it is a diagnostic bucket with its economic columns suppressed, so it has
	// no rank to take part in.
	let displaySections = $derived([
		...sections.filter((section) => section.yieldTier !== 'unknown'),
		...sections.filter((section) => section.yieldTier === 'unknown'),
	]);

	const signedPed = (value: number) => `${value >= 0 ? '+' : ''}${formatPed(value)}`;
	const netTone = (value: number) => (value >= 0 ? 'text-positive' : 'text-negative');
	const rateTone = (value: number) => netTone(value - 1);
	const sortArrow = (key: TreeCuttingActivitySortKey) =>
		sortKey === key ? (sortDir === 'asc' ? '\u2191' : '\u2193') : '';
	const sortDescription = (key: TreeCuttingActivitySortKey, label: string) => {
		if (sortKey !== key) return `Sort by ${label}`;
		return `Sort by ${label}, currently ${sortDir === 'asc' ? 'ascending' : 'descending'}`;
	};
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
	{@const isUnclassified = section.yieldTier === 'unknown'}
	<li>
		<button
			type="button"
			aria-pressed={isSelected}
			onclick={() => onselect(section.yieldTier)}
			class="w-full flex items-center gap-2 rounded-lg border px-3 py-2 text-left
				transition-[background-color,border-color] duration-[var(--duration-base)] ease-[var(--ease-out)]
				{isSelected
					? 'border-accent/40 bg-accent/[0.08]'
					: 'border-transparent hover:border-border/40 hover:bg-surface-hover/40'}"
		>
			<span
				class="flex-1 min-w-0 truncate text-sm font-medium tracking-tight
					{isUnclassified ? 'text-text-tertiary' : 'text-text'}"
				title={treeCuttingActivityName(section)}
			>
				{treeCuttingActivityName(section)}
			</span>
			{#if isUnclassified}
				<span class="sr-only">Activity metrics not applicable</span>
				<span class="w-14 shrink-0" aria-hidden="true"></span>
				<span class="w-16 shrink-0" aria-hidden="true"></span>
				<span class="w-[4.5rem] shrink-0" aria-hidden="true"></span>
			{:else}
				<span class="w-14 shrink-0 text-right text-xs tabular-nums text-text">
					{formatPed(section.cycled)}
				</span>
				<span class="w-16 shrink-0 text-right text-xs tabular-nums text-text">
					{section.muRate !== null ? formatPercent(section.muRate) : NO_DATA}
				</span>
				<span
					class="w-[4.5rem] shrink-0 text-right text-xs tabular-nums font-medium {rateTone(section.realisedRate)}"
				>
					{formatPercent(section.realisedRate)}
				</span>
			{/if}
		</button>
	</li>
{/snippet}

<Card class="hover:z-20">
	<div class="grid sm:grid-cols-[minmax(21rem,40%)_minmax(0,1fr)]">
		<div class="min-w-0 border-b border-border/40 sm:border-b-0 sm:border-r">
			<div class="px-2 pt-4">
				<div
					class="flex items-center gap-2 rounded-lg border border-transparent px-3 pb-2 text-text-tertiary"
				>
					<button
						type="button"
						class="eyebrow flex min-w-0 flex-1 cursor-pointer items-center gap-1 text-left transition-colors duration-[var(--duration-fast)] hover:text-text"
						aria-label={sortDescription('yieldTier', 'Activity')}
						onclick={() => onsort('yieldTier')}
					>
						Activity
						{#if sortKey === 'yieldTier'}<span class="text-accent">{sortArrow('yieldTier')}</span>{/if}
					</button>
					<button
						type="button"
						class="eyebrow flex w-14 shrink-0 cursor-pointer items-center justify-end gap-1 text-right transition-colors duration-[var(--duration-fast)] hover:text-text"
						aria-label={sortDescription('cycled', 'Cycled')}
						onclick={() => onsort('cycled')}
					>
						Cycled
						{#if sortKey === 'cycled'}<span class="text-accent">{sortArrow('cycled')}</span>{/if}
					</button>
					<button
						type="button"
						class="eyebrow flex w-16 shrink-0 cursor-pointer items-center justify-end gap-1 text-right transition-colors duration-[var(--duration-fast)] hover:text-text"
						aria-label={sortDescription('muRate', 'MU Rate')}
						onclick={() => onsort('muRate')}
					>
						MU Rate
						{#if sortKey === 'muRate'}<span class="text-accent">{sortArrow('muRate')}</span>{/if}
					</button>
					<button
						type="button"
						class="eyebrow flex w-[4.5rem] shrink-0 cursor-pointer items-center justify-end gap-1 text-right transition-colors duration-[var(--duration-fast)] hover:text-text"
						aria-label={sortDescription('realisedRate', 'Realised Rate')}
						onclick={() => onsort('realisedRate')}
					>
						Realised Rate
						{#if sortKey === 'realisedRate'}<span class="text-accent">{sortArrow('realisedRate')}</span>{/if}
					</button>
				</div>
			</div>
			<ul class="flex max-h-[32rem] flex-col gap-1 overflow-y-auto px-2 pb-3">
				{#each displaySections as section (section.yieldTier)}
					{@render subActivityRow(section, section.yieldTier === selected?.yieldTier)}
				{/each}
			</ul>
		</div>

		{#if selected}
			<div class="min-w-0 p-5">
				{#if selected.yieldTier === 'unknown'}
					<div class="flex min-h-28 items-center justify-center">
						<div class="flex items-center gap-1.5 text-sm text-text-secondary">
							<span>
								{selected.swings}
								{selected.swings === 1 ? 'swing is' : 'swings are'} unclassified and cannot be
								assigned to a board activity.
							</span>
							<InfoTip label="Why swings can be unclassified" width="w-80">
								<p class="text-xs font-semibold leading-relaxed text-text">
									Why swings can be unclassified
								</p>
								<p class="mt-1 text-xs leading-relaxed text-text-secondary">
									A swing is unclassified when no board output identifies its activity. This can
									happen on a failed or shavings-only swing without nearby board evidence from the
									same tool and hotkey run, when neighbouring evidence conflicts, or when a board
									name is not recognised.
								</p>
								<p class="mt-2 text-xs leading-relaxed text-text-tertiary">
									Its recorded cost and loot still count in Overall. They cannot be assigned to
									Short Boards, Boards, or Long Boards, so a large unclassified count makes the
									activity comparison less complete.
								</p>
							</InfoTip>
						</div>
					</div>
				{:else}
				<div class="grid grid-cols-3 gap-x-5">
					<StatDisplay
						label="TT Net"
						value={signedPed(selected.returns - selected.cycled)}
						unit="PED"
					/>
					<StatDisplay
						label="MU Net"
						value={selected.muProjectedReturns !== null
							? signedPed(selected.muProjectedReturns - selected.cycled)
							: NO_DATA}
						unit={selected.muProjectedReturns !== null ? 'PED' : ''}
					/>
					<StatDisplay
						label="Realised Net"
						value={signedPed(selected.realisedReturns - selected.cycled)}
						valueClass={netTone(selected.realisedReturns - selected.cycled)}
						unit="PED"
					>
						{#snippet labelSuffix()}
							<InfoTip align="right" width="w-80" label="What Realised Net reports">
								<p class="text-xs font-semibold leading-relaxed text-text">
									Realised Net: what this activity actually achieved
								</p>
								<p class="mt-1 text-xs leading-relaxed text-text-secondary">
									Loot TT less cycled PED, plus the markup confirmed sales of this activity's
									output have realised, after auction fees.
								</p>
								<p class="mt-2 text-xs leading-relaxed text-text-tertiary">
									It reads the same as TT Net until stock this activity produced is sold and the
									sale confirmed, because until then no markup has been realised. A sale recorded
									directly in the Ledger carries no link to an activity and does not reach here.
								</p>
							</InfoTip>
						{/snippet}
					</StatDisplay>
				</div>

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
						No loot recorded for this board activity yet.
					</p>
				{/if}
				{/if}
			</div>
		{/if}
	</div>
</Card>
