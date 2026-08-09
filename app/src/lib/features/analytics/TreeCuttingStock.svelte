<script lang="ts">
	import InfoTip from '$lib/components/InfoTip.svelte';
	import SearchInput from '$lib/components/SearchInput.svelte';
	import type { TreeCuttingStock } from './treeCuttingModel.svelte';
	import { NO_DATA, formatPed, formatPercent } from '$lib/utils/format';

	let {
		stock,
		onsell,
		onconvert,
		// The one activity-specific line in the panel, so the Hunting tab can
		// host the same surface over its own loot without forking the layout.
		sourceDescription = 'Loot recorded from tree cutting, minus loot you have already sold or converted.',
	}: {
		stock: TreeCuttingStock[];
		onsell: (item: TreeCuttingStock) => void;
		onconvert: (item: TreeCuttingStock) => void;
		sourceDescription?: string;
	} = $props();

	// A hunting loot table runs to hundreds of distinct items where a
	// harvesting one holds a handful, so the panel scales itself: past the
	// threshold a search appears and emptied lines fold behind a quiet
	// disclosure. Below it, nothing changes: every line stays visible,
	// emptied ones dimmed, exactly as this panel has always read.
	const SEARCH_THRESHOLD = 8;
	let query = $state('');
	let showEmptied = $state(false);
	const longList = $derived(stock.length > SEARCH_THRESHOLD);
	const matches = $derived(
		query.trim() === ''
			? stock
			: stock.filter((item) => item.itemName.toLowerCase().includes(query.trim().toLowerCase())),
	);
	// A live query overrides the emptied fold: a deliberate search is a
	// deliberate request for that item, sold out or not. The fold's count
	// reads against the whole list so the two can never disagree.
	const emptiedCount = $derived(stock.filter((item) => item.heldQty <= 0).length);
	const visibleStock = $derived(
		longList && !showEmptied && query.trim() === ''
			? matches.filter((item) => item.heldQty > 0)
			: matches,
	);

	function formatVolume(value: number): string {
		if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M`;
		if (value >= 1_000) return `${(value / 1_000).toFixed(1)}K`;
		return formatPed(value);
	}
	const marketPeriod = (horizon: string) =>
		horizon === 'week' || horizon === 'month' || horizon === 'year'
			? `last ${horizon}`
			: `over the last ${horizon}`;
	const marketShare = (value: number) => {
		const percent = value * 100;
		return percent < 0.1 ? 'less than 0.1%' : `${percent.toFixed(1)}%`;
	};
	const confidenceTitle = (tier: TreeCuttingStock['tier']) => {
		if (tier === 'liquid') return 'High markup confidence: This markup should be practical to realise';
		if (tier === 'middling') {
			return 'Medium markup confidence: It may be difficult to realise this markup';
		}
		return 'Low markup confidence: Do not rely on realising this markup';
	};
	const markupLabel = (item: TreeCuttingStock) => {
		if (item.effectiveMarkupPct == null) return 'No markup available';
		if (item.floored && item.markupPct !== null) {
			return `Observed markup ${formatPercent(item.markupPct / 100)}; projections use ${formatPercent(item.effectiveMarkupPct / 100)} Nanocube markup`;
		}
		if (item.markupPct == null) {
			return `Projections use ${formatPercent(item.effectiveMarkupPct / 100)} Nanocube markup`;
		}
		return `Markup ${formatPercent(item.effectiveMarkupPct / 100)}`;
	};

	function confidenceTip(item: TreeCuttingStock): {
		title: string;
		subtitle: string;
		example?: string;
		note?: string;
	} {
		if (item.markupPct == null || !item.opportunity) {
			return {
				title: confidenceTitle(item.tier),
				subtitle: 'No market MU is available for this item.',
			};
		}

		const horizon = item.markupHorizon;
		const salesPed = item.salesPed;
		let lead = 'No recent sales data is available for this item.';
		if (horizon && salesPed !== null) {
			if (horizon === 'week') {
				lead = `${formatPed(salesPed)} PED TT sold last week at ${formatPercent(item.markupPct / 100)} MU.`;
			} else {
			const weeklyReading =
				item.weeklySalesPed == null || item.weeklySalesPed <= 0
					? 'No sales in the last week.'
					: `${formatPed(item.weeklySalesPed)} PED TT sold last week.`;
			lead = `${weeklyReading} The current ${formatPercent(item.markupPct / 100)} MU comes from ${formatPed(salesPed)} PED TT sold ${marketPeriod(horizon)}.`;
			}
		}

		const batchTt = item.opportunity.efficientBatchTt;
		const batchShare = item.opportunity.efficientBatchMarketShare;
		const batchMarkup =
			batchTt === null ? null : batchTt * Math.max(0, item.markupPct / 100 - 1);
		const example =
			batchTt !== null && batchMarkup !== null && batchShare !== null && horizon
				? `For example: A ${formatPed(batchTt)} PED TT sale at this MU would produce about ${formatPed(batchMarkup)} PED of markup. The minimum auction fee is 0.5 PED, or 10% of that markup. That sale would be ${marketShare(batchShare)} of the TT sold ${marketPeriod(horizon)}.`
				: undefined;
		return {
			title: confidenceTitle(item.tier),
			subtitle: lead,
			example,
			note: example
				? undefined
				: 'The recorded MU does not provide enough markup to calculate a sale after fees.',
		};
	}
</script>

{#snippet actionButton(
	letter: string,
	label: string,
	onclick: () => void,
	disabled = false,
	title = '',
)}
	<!-- The expanded width comes from the label rather than a number chosen to
		suit it: `ch` is the font's own digit advance, so this tracks the type it
		is measuring and cannot be left behind by a rename. Lowercase letters run
		narrower than a digit, so it errs on the side of fitting. -->
	<button
		type="button"
		{onclick}
		{disabled}
		{title}
		aria-label={label}
		style="--expanded: calc({label.length}ch + 1.25rem)"
		class="group/act relative inline-flex h-6 w-6 shrink-0 items-center justify-center overflow-hidden
			rounded-md border border-border/40 bg-transparent text-xs font-semibold text-text-secondary
			transition-[width,color,border-color] duration-[var(--duration-base)] ease-[var(--ease-out)]
			hover:w-[var(--expanded)] hover:text-text hover:border-border
			disabled:cursor-not-allowed disabled:text-text-tertiary disabled:border-dashed
			disabled:hover:text-text-tertiary disabled:hover:border-border/40"
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

{#snippet confidenceBody(item: TreeCuttingStock)}
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

<div>
	<div
		class="flex flex-wrap items-center justify-between gap-x-4 gap-y-2 pb-2"
		data-testid="stock-utility-strip"
	>
		<div class="flex items-center gap-2">
			<h3 class="text-sm font-semibold tracking-tight text-text">Your Current Stock</h3>
			<InfoTip align="right" label="What current stock means">
				<div class="space-y-2 text-xs leading-relaxed text-text-secondary">
					<p class="font-semibold text-text">
						Your Current Stock: Loot you still hold
					</p>
					<p>
						{sourceDescription}
					</p>
					<p>
						Stock TT is its Trade Terminal value. Market markup only becomes a realised gain when a
						sale is confirmed.
					</p>
				</div>
			</InfoTip>
		</div>

		{#if longList}
			<SearchInput
				class="w-full sm:ml-auto sm:w-64"
				bind:value={query}
				placeholder="Find an item"
				aria-label="Find an item"
			/>
		{/if}
	</div>

	<div class="flex items-center gap-3 px-2.5 pb-1 text-text-tertiary">
		<span class="eyebrow flex-1 min-w-0">Item</span>
		<span class="eyebrow w-24 text-right shrink-0">Stock TT</span>
		<span class="eyebrow w-20 text-right shrink-0">Markup</span>
		<span class="eyebrow w-12 text-center shrink-0">Conf</span>
		<span class="eyebrow w-[3.375rem] shrink-0 text-right">Actions</span>
	</div>

	<ul class="flex max-h-[24rem] flex-col gap-1 overflow-y-auto">
		{#each visibleStock as item (item.itemName)}
			<!-- An emptied line stays, dimmed: the item is still one this
				activity produces, and its market reading is worth keeping
				legible for the next time there is stock to sell. -->
			{@const empty = item.heldQty <= 0}
			<li class="flex items-center gap-3 rounded-md px-2.5 py-2">
				<span
					class="flex-1 min-w-0 text-sm font-medium truncate tracking-tight
						{empty ? 'text-text-tertiary' : 'text-text'}"
				>
					{item.itemName}
				</span>

				<span
					class="w-24 text-right shrink-0 text-sm tabular-nums font-medium
						{empty ? 'text-text-tertiary' : 'text-text'}"
				>
					{formatPed(item.heldTt)}
					{#if item.listedQty > 0}
						<span
							class="block text-[0.625rem] font-normal text-text-tertiary tabular-nums"
							title="Out on an open auction; returns to stock if the listing expires"
						>
							{item.listedQty} listed
						</span>
					{/if}
				</span>

				<div class="w-20 shrink-0 flex items-center justify-end">
					{#if item.effectiveMarkupPct !== null && item.markupPct !== null}
						{@const observedMarkup = item.markupPct}
						{@const appliedMarkup = item.effectiveMarkupPct}
						<InfoTip align="right" width="w-96" label={markupLabel(item)}>
							{#snippet trigger()}
								<span
									class="inline-flex h-5 flex-col items-end justify-center tabular-nums
										border-b border-dotted border-border/70"
								>
									{#if item.floored}
										<span class="text-[9px] leading-[9px] text-text-tertiary line-through">
											{formatPercent(observedMarkup / 100)}
										</span>
										<span class="text-xs leading-[11px] text-text-secondary">
											{formatPercent(appliedMarkup / 100)}
										</span>
									{:else}
										<span class="text-sm leading-5 text-text-secondary">
											{formatPercent(appliedMarkup / 100)}
										</span>
									{/if}
								</span>
							{/snippet}
							{@render markupBreakdown(item.readings)}
						</InfoTip>
					{:else if item.effectiveMarkupPct !== null}
						{@const appliedMarkup = item.effectiveMarkupPct}
						<span
							class="inline-flex h-5 items-center text-sm tabular-nums text-text-secondary"
							aria-label={markupLabel(item)}
						>
							{formatPercent(appliedMarkup / 100)}
						</span>
					{:else}
						<span class="text-sm text-text-tertiary">{NO_DATA}</span>
					{/if}
				</div>

				<div class="w-12 shrink-0 flex items-center justify-center">
					{#if item.tier}
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
					{:else}
						<span class="text-sm text-text-tertiary">{NO_DATA}</span>
					{/if}
				</div>

				<div class="shrink-0 flex items-center justify-end gap-1.5">
					{@render actionButton(
						'N',
						'Nanocube',
						() => onconvert(item),
						item.heldQty <= 0,
						item.heldQty <= 0 ? 'Nothing held to convert' : '',
					)}
					{@render actionButton(
						'S',
						'Sell',
						() => onsell(item),
						item.heldQty <= 0,
						item.heldQty <= 0 ? 'Nothing held to sell' : '',
					)}
				</div>
			</li>
		{/each}
		{#if visibleStock.length === 0 && query.trim() !== ''}
			<li class="px-2.5 py-3 text-center text-xs text-text-tertiary">
				No stock item matches that search.
			</li>
		{/if}
	</ul>

	{#if longList && emptiedCount > 0 && query.trim() === ''}
		<button
			type="button"
			class="mt-1 px-2.5 text-xs text-text-tertiary cursor-pointer
				transition-colors duration-[var(--duration-fast)] hover:text-text"
			onclick={() => (showEmptied = !showEmptied)}
		>
			{showEmptied
				? 'Hide emptied items'
				: `Show ${emptiedCount} emptied ${emptiedCount === 1 ? 'item' : 'items'}`}
		</button>
	{/if}
</div>
