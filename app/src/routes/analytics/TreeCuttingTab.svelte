<script lang="ts">
	import Card from '$lib/components/Card.svelte';
	import DataTable from '$lib/components/DataTable.svelte';
	import ErrorNotice from '$lib/components/ErrorNotice.svelte';
	import {
		createTreeCuttingModel,
		MU_RATE_KEY,
		toolColumns
	} from '$lib/features/analytics/treeCuttingModel.svelte';
	import type { HarvestToolComparison } from '$lib/types/analytics';
	import { formatPed, formatPercent } from '$lib/utils/format';

	const model = createTreeCuttingModel();

	$effect(() => {
		void model.loadData();
	});
</script>

{#snippet toolCell({ column, value }: { column: { key: string }; value: unknown })}
	{#if column.key === 'cycled'}
		<span class="tabular-nums">{formatPed(Number(value))}</span>
	{:else if column.key === 'lootRate'}
		<span class="tabular-nums">{formatPercent(Number(value))}</span>
	{:else if column.key === MU_RATE_KEY}
		<span class="text-text-tertiary" title="Arrives with market data">&ndash;</span>
	{:else}
		{value}
	{/if}
{/snippet}

{#if model.loading}
	<p class="text-sm text-text-secondary">Loading tree cutting data...</p>
{:else if model.error && !model.data}
	<ErrorNotice message={model.error} />
{:else if model.data && model.data.toolComparisons.length > 0}
	<div class="space-y-6" data-guide-anchor="analytics-treecutting-area">
		<ErrorNotice message={model.error} />

		<!-- Per-tool comparison -->
		<div>
			<h3 class="eyebrow mb-3">Per-Tool Comparison</h3>
			<DataTable
				columns={toolColumns}
				rows={model.sortedTools}
				bind:sortKey={model.toolSortKey}
				bind:sortDir={model.toolSortDir}
				cell={toolCell}
				fixedLayout={true}
				rowKeyFn={(r: HarvestToolComparison) => r.toolName}
				emptyMessage="No tree cutting data available"
			/>
		</div>

		<div class="space-y-1 text-xs text-text-tertiary">
			<p>
				<span class="text-text-secondary">Rate:</span>
				loot-only TT return per cycled PED on that tool.
			</p>
			<p>
				<span class="text-text-secondary">MU Rate:</span>
				markup-adjusted return; populates once market data is connected.
			</p>
		</div>
	</div>
{:else}
	<Card class="p-6">
		<p class="text-sm text-text-tertiary text-center" data-guide-anchor="analytics-treecutting-area">
			No tree cutting data yet. Harvest trees during a tracked session to see per-tool comparisons.
		</p>
	</Card>
{/if}
