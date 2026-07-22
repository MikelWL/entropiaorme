<script lang="ts">
	import Card from '$lib/components/Card.svelte';
	import ErrorNotice from '$lib/components/ErrorNotice.svelte';
	import InfoTip from '$lib/components/InfoTip.svelte';
	import SegmentedControl from '$lib/components/SegmentedControl.svelte';
	import StatDisplay from '$lib/components/StatDisplay.svelte';
	import {
		type ConfidenceMode,
		createTreeCuttingModel,
		type TreeCuttingItem,
	} from '$lib/features/analytics/treeCuttingModel.svelte';
	import { formatPed, formatPercent } from '$lib/utils/format';

	const model = createTreeCuttingModel();

	$effect(() => {
		void model.loadData();
	});

	const NO_DATA = '—';

	// User-facing framing is trading-volume, not the internal tier names:
	// each option widens which items' own markup is trusted by how readily
	// the market can absorb the player's looted position.
	const MODE_OPTIONS: { id: ConfidenceMode; label: string }[] = [
		{ id: 'liquid', label: 'High Vol. Only' },
		{ id: 'liquidMiddling', label: 'High & Mid Vol.' },
		{ id: 'all', label: 'High, Mid & Low Vol.' },
	];

	// Whether a section carries market context at all (null MU = the
	// market feed was unavailable, so markup cells stay blank).
	function hasMarket(mu: number | null): boolean {
		return mu !== null;
	}

	function confidenceTooltip(item: TreeCuttingItem): string {
		const pos = `your position ${formatPed(item.positionTt)} PED`;
		const vol =
			item.weeklyEquivVolume > 0
				? `~${formatPed(item.weeklyEquivVolume)} PED/wk traded`
				: 'no recent sales';
		let reason: string;
		if (item.ownMarkupPct == null) {
			reason = 'No market observation for this item.';
		} else if (item.tier === 'liquid') {
			reason = `High volume: ${pos} is a small share of ${vol}.`;
		} else if (item.tier === 'middling') {
			reason = `Medium volume: ${pos} vs ${vol}${
				item.markupHorizon && item.markupHorizon !== 'week'
					? ` (markup from the ${item.markupHorizon} horizon)`
					: ''
			}.`;
		} else {
			reason = `Low volume: ${pos} vs ${vol} may not sell at this markup.`;
		}
		if (item.floored) {
			reason += ' Showing the nanocube recycling floor instead.';
		}
		return reason;
	}
</script>

{#if model.loading}
	<p class="text-sm text-text-secondary">Loading tree cutting data...</p>
{:else if model.error && !model.sections.length}
	<ErrorNotice message={model.error} />
{:else if model.sections.length > 0}
	<div class="space-y-5" data-guide-anchor="analytics-treecutting-area">
		<ErrorNotice message={model.error} />

		<!-- Markup-confidence toggle: right-hung, explanation behind an info tip -->
		<div class="flex items-center justify-end gap-2.5">
			<span class="eyebrow">Markup confidence</span>
			<InfoTip label="How markup confidence works">
				<div class="space-y-2 text-xs leading-relaxed text-text-secondary">
					<p class="text-text">
						Which items' market markup is trusted in the MU figures, by whether the
						market's <span class="text-text">trading volume</span> can absorb your looted
						position at that markup.
					</p>
					<ul class="space-y-1.5">
						<li>
							<span class="text-text font-medium">High Vol.</span> weekly volume easily
							covers your position: the markup is realistically achievable.
						</li>
						<li>
							<span class="text-text font-medium">Mid Vol.</span> your position is a
							sizeable share of volume: selling it all at this markup is uncertain.
						</li>
						<li>
							<span class="text-text font-medium">Low Vol.</span> thinly traded: your
							position would flood the market and the markup is unlikely to hold.
						</li>
					</ul>
					<p>
						Items outside your choice fall back to the nanocube recycling floor: a
						TT-neutral conversion any item can realise, shown struck through with the
						floor value.
					</p>
				</div>
			</InfoTip>
			<SegmentedControl
				options={MODE_OPTIONS}
				active={model.confidenceMode}
				onchange={(id) => (model.confidenceMode = id as ConfidenceMode)}
			/>
		</div>

		{#each model.sections as section (section.toolName)}
			<Card class="p-5">
				<!-- Stat area as a 2x3 grid: the title occupies the top-left
					cell as the box's heading, MU figures fill out row 1, and
					the realised stats sit in row 2. -->
				<div class="grid grid-cols-2 gap-x-4 gap-y-4 sm:grid-cols-3">
					<div
						class="col-span-2 sm:col-span-1 flex flex-col justify-center gap-0.5
							rounded-lg border-l-2 border-accent bg-accent/[0.05] px-3.5 py-2"
					>
						<span class="text-lg font-semibold tracking-tight leading-tight text-text">
							{section.tree ? `${section.tree} Trees` : section.toolName}
						</span>
						{#if section.tree}
							<span class="text-xs text-text-secondary">{section.toolName}</span>
						{/if}
					</div>

					<StatDisplay
						label="MU Proj. Returns"
						value={section.muProjectedReturns !== null
							? formatPed(section.muProjectedReturns)
							: NO_DATA}
						unit={section.muProjectedReturns !== null ? 'PED' : ''}
					/>
					<StatDisplay
						label="MU Rate"
						value={section.muRate !== null ? formatPercent(section.muRate) : NO_DATA}
					/>

					<StatDisplay label="Cycled" value={formatPed(section.cycled)} unit="PED" />
					<StatDisplay label="Returns" value={formatPed(section.returns)} unit="PED" />
					<StatDisplay label="Rate" value={formatPercent(section.lootRate)} />
				</div>

				<!-- Per-item breakdown -->
				{#if section.items.length > 0}
					<div class="mt-5 border-t border-border/50 pt-4">
						<div class="flex items-center gap-3 px-2.5 pb-1 text-text-tertiary">
							<span class="eyebrow flex-1 min-w-0">Item</span>
							<span class="eyebrow w-20 text-right shrink-0">TT</span>
							<span class="eyebrow w-14 text-right shrink-0">Share</span>
							<span class="eyebrow w-36 text-right shrink-0">Markup</span>
						</div>

						<ul class="flex flex-col gap-1">
							{#each section.items as item (item.name)}
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

									<!-- Markup: neutral number + a separate confidence glyph;
										floored markups are struck through and shown at the
										nanocube recycling rate. -->
									<span
										class="text-sm tabular-nums shrink-0 w-36 text-right flex items-center justify-end gap-1.5"
										title={hasMarket(section.muProjectedReturns) ? confidenceTooltip(item) : ''}
									>
										{#if !hasMarket(section.muProjectedReturns)}
											<span class="text-text-tertiary">{NO_DATA}</span>
										{:else}
											{#if item.tier === 'middling'}
												<span class="text-warning shrink-0" aria-label="Medium volume">⚠</span>
											{:else if item.tier === 'illiquid'}
												<span class="text-error font-semibold shrink-0" aria-label="Low volume">!</span>
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
			</Card>
		{/each}

		<div class="space-y-1 text-xs text-text-tertiary">
			<p>
				<span class="text-text-secondary">Rate:</span>
				loot-only TT return per cycled PED on that tool.
			</p>
			<p>
				<span class="text-text-secondary">MU figures:</span>
				estimated from market data, never realised P&L. Markup resolves from the weekly
				horizon (falling back to monthly, then yearly). A
				<span class="text-warning">⚠</span> flags a markup the market may only partly absorb;
				a <span class="text-error font-semibold">!</span> flags one that likely cannot be sold
				at that rate, shown struck through with the nanocube recycling floor.
			</p>
		</div>
	</div>
{:else}
	<Card class="p-6">
		<p class="text-sm text-text-tertiary text-center" data-guide-anchor="analytics-treecutting-area">
			No tree cutting data yet. Harvest trees during a tracked session to see per-tool sections.
		</p>
	</Card>
{/if}
