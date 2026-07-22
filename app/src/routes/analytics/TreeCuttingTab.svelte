<script lang="ts">
	import Card from '$lib/components/Card.svelte';
	import ErrorNotice from '$lib/components/ErrorNotice.svelte';
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

	const MODES: { value: ConfidenceMode; label: string }[] = [
		{ value: 'liquid', label: 'Liquid' },
		{ value: 'liquidMiddling', label: 'Liquid + Middling' },
		{ value: 'all', label: 'All' },
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
				? `market ~${formatPed(item.weeklyEquivVolume)} PED/wk`
				: 'no recent sales';
		let reason: string;
		if (item.ownMarkupPct == null) {
			reason = 'No market observation for this item.';
		} else if (item.tier === 'liquid') {
			reason = `Liquid: ${pos} is a small share of ${vol}.`;
		} else if (item.tier === 'middling') {
			reason = `Moderate confidence: ${pos} vs ${vol}${
				item.markupHorizon && item.markupHorizon !== 'week'
					? ` (markup from the ${item.markupHorizon} horizon)`
					: ''
			}.`;
		} else {
			reason = `Low confidence: ${pos} vs ${vol} may not sell at this markup.`;
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
	<div class="space-y-6" data-guide-anchor="analytics-treecutting-area">
		<ErrorNotice message={model.error} />

		<!-- Markup-confidence toggle -->
		<div class="flex items-center gap-3 flex-wrap">
			<span class="eyebrow">Markup confidence</span>
			<div class="inline-flex rounded-md border border-border/60 overflow-hidden text-xs">
				{#each MODES as m (m.value)}
					<button
						type="button"
						class="px-3 py-1.5 transition-colors duration-[var(--duration-fast)]
							{model.confidenceMode === m.value
							? 'bg-surface-raised text-text'
							: 'text-text-tertiary hover:text-text-secondary hover:bg-surface-hover/40'}"
						aria-pressed={model.confidenceMode === m.value}
						onclick={() => (model.confidenceMode = m.value)}
					>
						{m.label}
					</button>
				{/each}
			</div>
			<span class="text-xs text-text-tertiary">
				sets which markups feed MU; the rest fall back to the nanocube recycling floor.
			</span>
		</div>

		{#each model.sections as section (section.toolName)}
			<Card class="p-5">
				<header class="mb-4">
					{#if section.tree}
						<h3 class="text-lg font-semibold tracking-tight text-text">
							{section.tree} Trees
						</h3>
						<p class="text-sm text-text-secondary">{section.toolName}</p>
					{:else}
						<h3 class="text-lg font-semibold tracking-tight text-text">
							{section.toolName}
						</h3>
					{/if}
				</header>

				<!-- Top strip: realised stats + estimated MU -->
				<div
					class="grid grid-cols-2 gap-x-4 gap-y-4 sm:grid-cols-3 lg:grid-cols-6 border-b border-border/50 pb-5"
				>
					<StatDisplay label="Swings" value={section.swings} />
					<StatDisplay label="Cycled" value={formatPed(section.cycled)} unit="PED" />
					<StatDisplay label="Returns" value={formatPed(section.returns)} unit="PED" />
					<StatDisplay label="Rate" value={formatPercent(section.lootRate)} />
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
				</div>

				<!-- Per-item breakdown -->
				{#if section.items.length > 0}
					<div class="mt-4">
						<div class="flex items-center gap-3 px-2.5 pb-1 text-text-tertiary">
							<span class="eyebrow flex-1 min-w-0">Item</span>
							<span class="hidden sm:block w-20 shrink-0"></span>
							<span class="eyebrow w-20 text-right shrink-0">TT</span>
							<span class="eyebrow w-14 text-right shrink-0">Share</span>
							<span class="eyebrow w-24 text-right shrink-0">Markup</span>
						</div>

						<ul class="flex flex-col gap-1">
							{#each section.items as item (item.name)}
								<li
									class="flex items-center gap-3 rounded-md px-2.5 py-2 border border-transparent
										hover:bg-surface-hover/30 hover:border-border/40
										transition-[background-color,border-color] duration-[var(--duration-base)] ease-[var(--ease-out)]"
								>
									<div class="flex-1 min-w-0 flex items-center gap-2">
										<span class="text-sm font-medium truncate tracking-tight text-text">
											{item.name}
										</span>
										<span class="text-xs text-text-tertiary tabular-nums shrink-0">
											×{item.quantity}
										</span>
									</div>

									<div class="hidden sm:block w-20 h-1 rounded-full bg-base/60 overflow-hidden shrink-0">
										<div
											class="h-full rounded-full bg-accent transition-[width] duration-[var(--duration-slow)] ease-[var(--ease-out)]"
											style="width: {item.sharePct}%;"
										></div>
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
										class="text-sm tabular-nums shrink-0 w-24 text-right flex items-center justify-end gap-1"
										title={hasMarket(section.muProjectedReturns) ? confidenceTooltip(item) : ''}
									>
										{#if !hasMarket(section.muProjectedReturns)}
											<span class="text-text-tertiary">{NO_DATA}</span>
										{:else}
											{#if item.tier === 'middling'}
												<span class="text-warning" aria-label="Moderate confidence">⚠</span>
											{:else if item.tier === 'illiquid'}
												<span class="text-error font-semibold" aria-label="Low confidence">!</span>
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
				<span class="text-text-secondary">MU Proj. Returns / MU Rate / Markup:</span>
				estimated from market data, never realised P&L. Markup resolves from the weekly
				horizon (falling back to monthly, then yearly). A
				<span class="text-warning">⚠</span> flags a markup the market may not fully absorb; a
				<span class="text-error font-semibold">!</span> flags one that likely cannot be sold at
				that rate, where the realistic value is the nanocube recycling floor.
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
