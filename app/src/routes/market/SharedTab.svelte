<script lang="ts">
	import Button from '$lib/components/Button.svelte';
	import Card from '$lib/components/Card.svelte';
	import DataTable from '$lib/components/DataTable.svelte';
	import ErrorNotice from '$lib/components/ErrorNotice.svelte';
	import SearchInput from '$lib/components/SearchInput.svelte';
	import SegmentedControl from '$lib/components/SegmentedControl.svelte';
	import { goto } from '$app/navigation';
	import type { MarketHorizon } from '$lib/api';
	import type { SharedTableRow } from '$lib/features/market/sharedModel.svelte';
	import { createSharedModel, isoToEpoch } from '$lib/features/market/sharedModel.svelte';
	import { formatAge, formatSalesPed, HORIZONS } from '$lib/features/market/overviewModel.svelte';

	const model = createSharedModel();

	const nowEpoch = Date.now() / 1000;

	const columns = [
		{ key: 'itemName', label: 'Item' },
		{ key: 'markupPct', label: 'Markup', align: 'right' as const },
		{ key: 'salesPed', label: 'Sales volume', align: 'right' as const },
		{ key: 'observedAt', label: 'Observed', align: 'right' as const }
	];
</script>

{#if !model.fetchEnabled}
	<Card>
		<div class="py-10 text-center space-y-2">
			<p class="text-sm text-text-secondary">Market data is turned off.</p>
			<p class="text-sm text-text-tertiary">
				Enable it in
				<button class="linklet" type="button" onclick={() => goto('/settings')}>Settings</button>
				to fetch the shared snapshot.
			</p>
		</div>
	</Card>
{:else}
	{#if model.refreshError}
		<div class="mb-4"><ErrorNotice message={model.refreshError} /></div>
	{/if}
	<Card>
		<div class="flex items-center justify-between gap-3 mb-4 flex-wrap">
			<div class="flex items-center gap-3">
				<SearchInput bind:value={model.search} placeholder="Filter items" class="max-w-56" />
				<SegmentedControl
					options={HORIZONS}
					active={model.horizon}
					onchange={(id) => (model.horizon = id as MarketHorizon)}
				/>
			</div>
			<div class="flex items-center gap-2">
				{#if model.contributionEnabled}
					<Button
						variant="secondary"
						disabled={model.contributing}
						onclick={() => void model.contributeLatest()}
					>
						{model.contributing ? 'Sending' : 'Send latest paste'}
					</Button>
				{/if}
				<Button
					variant="secondary"
					disabled={model.refreshing}
					onclick={() => void model.refresh()}
				>
					{model.refreshing ? 'Refreshing' : 'Refresh'}
				</Button>
			</div>
		</div>
		{#if model.contributionNote}
			<p class="text-xs text-text-tertiary mb-3">{model.contributionNote}</p>
		{/if}
		{#if model.cache}
			<p class="text-xs text-text-tertiary mb-3">
				Snapshot generated {formatAge(isoToEpoch(model.cache.generatedAt), nowEpoch)}
				· fetched {formatAge(isoToEpoch(model.cache.fetchedAt), nowEpoch)}
				· {model.cache.items.length} items
				· {model.cache.contributorCount} contributor{model.cache.contributorCount === 1
					? ''
					: 's'}
			</p>
			<DataTable
				{columns}
				rows={model.tableRows}
				emptyMessage="No items match the filter"
			>
				{#snippet cell({ row, column }: { row: SharedTableRow; column: { key: string } })}
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
		{:else}
			<div class="py-10 text-center space-y-2">
				<p class="text-sm text-text-secondary">No shared snapshot fetched yet.</p>
				<p class="text-sm text-text-tertiary">
					Refresh to fetch the latest shared market snapshot.
				</p>
			</div>
		{/if}
	</Card>
{/if}
