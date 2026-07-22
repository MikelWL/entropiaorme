<script lang="ts">
	import Card from '$lib/components/Card.svelte';
	import ErrorNotice from '$lib/components/ErrorNotice.svelte';
	import InfoTip from '$lib/components/InfoTip.svelte';
	import SegmentedControl from '$lib/components/SegmentedControl.svelte';
	import TreeCuttingActivities from '$lib/features/analytics/TreeCuttingActivities.svelte';
	import TreeCuttingStats from '$lib/features/analytics/TreeCuttingStats.svelte';
	import TreeCuttingStock from '$lib/features/analytics/TreeCuttingStock.svelte';
	import {
		createTreeCuttingModel,
		type ConfidenceMode,
	} from '$lib/features/analytics/treeCuttingModel.svelte';

	const model = createTreeCuttingModel();

	$effect(() => {
		void model.loadData();
	});

	const MODE_OPTIONS: { id: ConfidenceMode; label: string }[] = [
		{ id: 'liquid', label: 'High Vol. Only' },
		{ id: 'liquidMiddling', label: 'High & Mid Vol.' },
		{ id: 'all', label: 'High, Mid & Low Vol.' },
	];
</script>

{#if model.loading}
	<p class="text-sm text-text-secondary">Loading tree cutting data...</p>
{:else if model.error && !model.sections.length}
	<ErrorNotice message={model.error} />
{:else if model.sections.length > 0}
	<div class="space-y-5" data-guide-anchor="analytics-treecutting-area">
		<ErrorNotice message={model.error} />

		<div class="flex items-center justify-end gap-2.5">
			<span class="eyebrow">Markup confidence</span>
			<InfoTip label="How markup confidence works">
				<div class="space-y-2 text-xs leading-relaxed text-text-secondary">
					<p class="text-text">
						Sets which items count toward the MU figures, based on market-wide markup, TT
						turnover, evidence horizon, and fee economics.
					</p>
					<ul class="space-y-1.5">
						<li>
							<span class="text-text font-medium">High Vol.</span> has broad weekly capacity.
						</li>
						<li>
							<span class="text-text font-medium">Mid Vol.</span> has sparse capacity but enough
							unit margin to amortise fees.
						</li>
						<li>
							<span class="text-text font-medium">Low Vol.</span> has a constrained direct market.
						</li>
					</ul>
					<p>
						Items left out are valued at the nanocube rate instead. The classification is
						independent of how much stock you currently hold.
					</p>
				</div>
			</InfoTip>
			<SegmentedControl
				options={MODE_OPTIONS}
				active={model.confidenceMode}
				onchange={(id) => (model.confidenceMode = id as ConfidenceMode)}
			/>
		</div>

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
						muProjectedReturns={model.overall.muProjectedReturns}
						muRate={model.overall.muRate}
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
				<span class="text-text-secondary">TT Net / TT Rate:</span>
				realised loot TT minus cycled PED, and loot-only TT return per cycled PED.
			</p>
			<p>
				<span class="text-text-secondary">MU Net / MU figures:</span>
				estimated from market data, never realised P&amp;L. Markup resolves from the weekly
				horizon (falling back to monthly, then yearly). A
				<span class="text-warning">⚠</span> flags a sparse but fee-viable market; a
				<span class="text-error font-semibold">!</span> flags constrained capacity, shown struck
				through with the nanocube rate when excluded by the confidence toggle.
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
