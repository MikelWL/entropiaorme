<script lang="ts">
	import Card from '$lib/components/Card.svelte';
	import ErrorNotice from '$lib/components/ErrorNotice.svelte';
	import StatDisplay from '$lib/components/StatDisplay.svelte';
	import { createTreeCuttingModel } from '$lib/features/analytics/treeCuttingModel.svelte';
	import { formatPed, formatPercent } from '$lib/utils/format';

	const model = createTreeCuttingModel();

	$effect(() => {
		void model.loadData();
	});

	// Placeholder marker for the market-derived cells until the MU feed
	// is merged in.
	const MU_PENDING = '—';
</script>

{#if model.loading}
	<p class="text-sm text-text-secondary">Loading tree cutting data...</p>
{:else if model.error && !model.sections.length}
	<ErrorNotice message={model.error} />
{:else if model.sections.length > 0}
	<div class="space-y-6" data-guide-anchor="analytics-treecutting-area">
		<ErrorNotice message={model.error} />

		{#each model.sections as section (section.toolName)}
			<Card class="p-5">
				<!-- Header: primary tree, then tool -->
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

				<!-- Top strip: realised stats + MU placeholders -->
				<div
					class="grid grid-cols-2 gap-x-4 gap-y-4 sm:grid-cols-3 lg:grid-cols-6 border-b border-border/50 pb-5"
				>
					<StatDisplay label="Swings" value={section.swings} />
					<StatDisplay label="Cycled" value={formatPed(section.cycled)} unit="PED" />
					<StatDisplay label="Returns" value={formatPed(section.returns)} unit="PED" />
					<StatDisplay label="Rate" value={formatPercent(section.lootRate)} />
					<StatDisplay label="MU Proj. Returns" value={MU_PENDING} />
					<StatDisplay label="MU Rate" value={MU_PENDING} />
				</div>

				<!-- Per-item breakdown -->
				{#if section.items.length > 0}
					<div class="mt-4">
						<!-- Column headers -->
						<div class="flex items-center gap-3 px-2.5 pb-1 text-text-tertiary">
							<span class="eyebrow flex-1 min-w-0">Item</span>
							<span class="hidden sm:block w-20 shrink-0"></span>
							<span class="eyebrow w-20 text-right shrink-0">TT</span>
							<span class="eyebrow w-14 text-right shrink-0">Share</span>
							<span class="eyebrow w-16 text-right shrink-0">Markup</span>
						</div>

						<ul class="flex flex-col gap-1">
							{#each section.items as item (item.name)}
								<li
									class="flex items-center gap-3 rounded-md px-2.5 py-2 border border-transparent
										hover:bg-surface-hover/30 hover:border-border/40
										transition-[background-color,border-color] duration-[var(--duration-base)] ease-[var(--ease-out)]"
								>
									<!-- Name + qty -->
									<div class="flex-1 min-w-0 flex items-center gap-2">
										<span class="text-sm font-medium truncate tracking-tight text-text">
											{item.name}
										</span>
										<span class="text-xs text-text-tertiary tabular-nums shrink-0">
											×{item.quantity}
										</span>
									</div>

									<!-- Mini share bar -->
									<div class="hidden sm:block w-20 h-1 rounded-full bg-base/60 overflow-hidden shrink-0">
										<div
											class="h-full rounded-full bg-accent transition-[width] duration-[var(--duration-slow)] ease-[var(--ease-out)]"
											style="width: {item.sharePct}%;"
										></div>
									</div>

									<!-- TT value -->
									<span class="text-sm tabular-nums font-medium text-text shrink-0 w-20 text-right">
										{formatPed(item.ttValue)}
									</span>

									<!-- Share -->
									<span
										class="text-sm tabular-nums font-semibold text-accent shrink-0 w-14 text-right tracking-tight"
									>
										{item.sharePct.toFixed(1)}%
									</span>

									<!-- Markup (arrives with market data) -->
									<span
										class="text-sm tabular-nums text-text-tertiary shrink-0 w-16 text-right"
										title="Arrives with market data"
									>
										{MU_PENDING}
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
				markup-adjusted figures; populate once market data is connected.
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
