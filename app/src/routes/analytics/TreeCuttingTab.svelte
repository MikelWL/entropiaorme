<script lang="ts">
	import Card from '$lib/components/Card.svelte';
	import ErrorNotice from '$lib/components/ErrorNotice.svelte';
	import TreeCuttingActivities from '$lib/features/analytics/TreeCuttingActivities.svelte';
	import TreeCuttingStats from '$lib/features/analytics/TreeCuttingStats.svelte';
	import TreeCuttingStock from '$lib/features/analytics/TreeCuttingStock.svelte';
	import { createTreeCuttingModel } from '$lib/features/analytics/treeCuttingModel.svelte';

	const model = createTreeCuttingModel();

	$effect(() => {
		void model.loadData();
	});
</script>

{#if model.loading}
	<p class="text-sm text-text-secondary">Loading tree cutting data...</p>
{:else if model.error && !model.sections.length}
	<ErrorNotice message={model.error} />
{:else if model.sections.length > 0}
	<div class="space-y-5" data-guide-anchor="analytics-treecutting-area">
		<ErrorNotice message={model.error} />

		{#if model.overall}
			<div
				class="relative hover:z-20 rounded-xl border border-accent/30 p-6 shadow-lg
					backdrop-blur-[2px] bg-gradient-to-br from-accent/[0.12] via-surface/70 to-surface/70"
			>
				<div class="grid gap-x-8 gap-y-6 sm:grid-cols-[auto_minmax(0,1fr)]">
					<TreeCuttingStats
						heading="Overall"
						cycled={model.overall.cycled}
						returns={model.overall.returns}
						lootRate={model.overall.lootRate}
						marketReturns={model.overall.marketReturns}
						marketRate={model.overall.marketRate}
						realisedReturns={model.overall.realisedReturns}
						realisedRate={model.overall.realisedRate}
					/>

					{#if model.stock.length > 0}
						<TreeCuttingStock stock={model.stock} />
					{/if}
				</div>
			</div>
		{/if}

		<TreeCuttingActivities
			sections={model.sections}
			selected={model.selectedSection}
			onselect={(toolName) => model.selectSection(toolName)}
		/>

		<div class="space-y-1 text-xs text-text-tertiary">
			<p>
				<span class="text-text-secondary">TT floor:</span>
				loot at Trade Terminal value, available without a market sale.
			</p>
			<p>
				<span class="text-accent">Current market:</span>
				today's holding-independent estimate for this observed output composition. Broad,
				niche, and thin describe different opportunity shapes, not guaranteed sale outcomes.
			</p>
			<p>
				<span class="text-positive">Realised:</span>
				loot TT plus markup from confirmed sales attributed to the activities that produced it.
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
