<script lang="ts">
	import Card from '$lib/components/Card.svelte';
	import DataTable from '$lib/components/DataTable.svelte';
	import ErrorNotice from '$lib/components/ErrorNotice.svelte';
	import SearchInput from '$lib/components/SearchInput.svelte';
	import SegmentedControl from '$lib/components/SegmentedControl.svelte';
	import type { MarketHorizon } from '$lib/api';
	import type { OverviewTableRow } from '$lib/features/market/overviewModel.svelte';
	import {
		createOverviewModel,
		formatAge,
		formatSalesPed,
		HORIZONS
	} from '$lib/features/market/overviewModel.svelte';

	const model = createOverviewModel();

	$effect(() => {
		void model.loadData();
	});

	// The staleness labels compare against load time; a table of
	// week-scale ages does not need a ticking clock.
	const nowEpoch = Date.now() / 1000;

	const columns = [
		{ key: 'itemName', label: 'Item', sortable: true },
		{ key: 'markupPct', label: 'Markup', align: 'right' as const, sortable: true },
		{ key: 'salesPed', label: 'Sales volume', align: 'right' as const, sortable: true },
		{ key: 'observedAt', label: 'Observed', align: 'right' as const, sortable: true }
	];
</script>

{#if model.error}
	<ErrorNotice message={model.error} />
{:else if !model.loading && model.rows.length === 0}
	<Card>
		<div class="py-10 text-center space-y-2">
			<p class="text-sm text-text-secondary">No market observations yet.</p>
			<p class="text-sm text-text-tertiary">
				Import your first market-ledger paste from the Import tab.
			</p>
		</div>
	</Card>
{:else}
	<Card>
		<div class="flex items-center justify-between gap-3 mb-4">
			<SearchInput bind:value={model.search} placeholder="Filter items" class="max-w-56" />
			<SegmentedControl
				options={HORIZONS}
				active={model.horizon}
				onchange={(id) => (model.horizon = id as MarketHorizon)}
			/>
		</div>
		<DataTable
			{columns}
			rows={model.sortedRows}
			bind:sortKey={model.sortKey}
			bind:sortDir={model.sortDir}
			emptyMessage={model.loading ? 'Loading market data' : 'No items match the filter'}
		>
			{#snippet cell({ row, column }: { row: OverviewTableRow; column: { key: string } })}
				{#if column.key === 'itemName'}
					<span class="text-text">{row.itemName}</span>
					{#if row.tier > 0}
						<span class="ml-1.5 text-xs text-text-tertiary">T{row.tier}</span>
					{/if}
				{:else if column.key === 'markupPct'}
					{#if row.markupPct === null}
						<span class="text-text-tertiary" title="No sales in this window">N/A</span>
					{:else}
						<span class="tabular-nums">{row.markupPct.toFixed(2)}%</span>
					{/if}
				{:else if column.key === 'salesPed'}
					<span class="tabular-nums text-text-secondary">{formatSalesPed(row.salesPed)}</span>
				{:else if column.key === 'observedAt'}
					<span class="text-text-tertiary">{formatAge(row.observedAt, nowEpoch)}</span>
				{/if}
			{/snippet}
		</DataTable>
	</Card>
{/if}
