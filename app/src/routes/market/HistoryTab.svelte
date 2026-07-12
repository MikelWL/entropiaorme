<script lang="ts">
	import Card from '$lib/components/Card.svelte';
	import DataTable from '$lib/components/DataTable.svelte';
	import ErrorNotice from '$lib/components/ErrorNotice.svelte';
	import SegmentedControl from '$lib/components/SegmentedControl.svelte';
	import Select from '$lib/components/Select.svelte';
	import type { MarketHistoryPoint, MarketHorizon } from '$lib/api';
	import { createHistoryModel } from '$lib/features/market/historyModel.svelte';
	import { formatSalesPed, HORIZONS } from '$lib/features/market/overviewModel.svelte';

	const model = createHistoryModel();

	$effect(() => {
		void model.loadItems();
	});

	const columns = [
		{ key: 'observedAt', label: 'Observed', align: 'left' as const },
		{ key: 'markupPct', label: 'Markup', align: 'right' as const },
		{ key: 'salesPed', label: 'Sales volume', align: 'right' as const }
	];

	function formatDate(epoch: number): string {
		return new Date(epoch * 1000).toLocaleDateString(undefined, {
			year: 'numeric',
			month: 'short',
			day: 'numeric'
		});
	}
</script>

{#if model.error}
	<ErrorNotice message={model.error} />
{:else if model.itemNames.length === 0}
	<Card>
		<div class="py-10 text-center space-y-2">
			<p class="text-sm text-text-secondary">No market observations yet.</p>
			<p class="text-sm text-text-tertiary">
				Import a market-ledger paste first; each import becomes a point in an item's history.
			</p>
		</div>
	</Card>
{:else}
	<Card>
		<div class="flex items-center justify-between gap-3 mb-4">
			<Select
				value={model.selectedItem}
				onchange={(e) => model.selectItem((e.currentTarget as HTMLSelectElement).value)}
				aria-label="Item"
				class="max-w-72"
			>
				{#each model.itemNames as name (name)}
					<option value={name}>{name}</option>
				{/each}
			</Select>
			<SegmentedControl
				options={HORIZONS}
				active={model.horizon}
				onchange={(id) => model.selectHorizon(id as MarketHorizon)}
			/>
		</div>
		<DataTable
			{columns}
			rows={model.points}
			emptyMessage={model.loading ? 'Loading history' : 'No observations for this item yet'}
		>
			{#snippet cell({ row, column }: { row: MarketHistoryPoint; column: { key: string } })}
				{#if column.key === 'observedAt'}
					<span class="text-text-secondary">{formatDate(row.observedAt)}</span>
				{:else if column.key === 'markupPct'}
					{#if row.markupPct === null}
						<span class="text-text-tertiary" title="No sales in this window">N/A</span>
					{:else}
						<span class="tabular-nums">{row.markupPct.toFixed(2)}%</span>
					{/if}
				{:else if column.key === 'salesPed'}
					<span class="tabular-nums text-text-secondary">{formatSalesPed(row.salesPed)}</span>
				{/if}
			{/snippet}
		</DataTable>
	</Card>
{/if}
