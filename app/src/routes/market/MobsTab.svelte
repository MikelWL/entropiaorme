<script lang="ts">
	import Card from '$lib/components/Card.svelte';
	import DataTable from '$lib/components/DataTable.svelte';
	import ErrorNotice from '$lib/components/ErrorNotice.svelte';
	import SegmentedControl from '$lib/components/SegmentedControl.svelte';
	import type { MarketHorizon, MarketMobRankingRow } from '$lib/api';
	import { coveragePct, createMobsModel } from '$lib/features/market/mobsModel.svelte';
	import { formatSalesPed, HORIZONS } from '$lib/features/market/overviewModel.svelte';

	const model = createMobsModel();

	$effect(() => {
		void model.loadData();
	});

	const columns = [
		{ key: 'mobSpecies', label: 'Mob' },
		{ key: 'estMarkupPct', label: 'Est. loot markup', align: 'right' as const },
		{ key: 'coveredTt', label: 'Coverage', align: 'right' as const },
		{ key: 'lootTt', label: 'Recorded loot', align: 'right' as const }
	];
</script>

{#if model.error}
	<ErrorNotice message={model.error} />
{:else if !model.loading && model.rows.length === 0}
	<Card>
		<div class="py-10 text-center space-y-2">
			<p class="text-sm text-text-secondary">No mob loot recorded yet.</p>
			<p class="text-sm text-text-tertiary">
				Track hunting sessions with a mob selected and this view ranks your mobs by estimated
				loot markup.
			</p>
		</div>
	</Card>
{:else}
	<Card>
		<div class="flex items-center justify-between gap-3 mb-4">
			<p class="text-sm text-text-secondary">
				Your hunted mobs ranked by estimated loot markup: recorded loot composition weighted by
				the latest observations. An estimate to steer by, never part of your recorded results.
			</p>
			<SegmentedControl
				options={HORIZONS}
				active={model.horizon}
				onchange={(id) => model.selectHorizon(id as MarketHorizon)}
			/>
		</div>
		<DataTable
			{columns}
			rows={model.rows}
			emptyMessage={model.loading ? 'Loading mob ranking' : 'No mob loot recorded yet'}
		>
			{#snippet cell({ row, column }: { row: MarketMobRankingRow; column: { key: string } })}
				{#if column.key === 'mobSpecies'}
					<span class="text-text">{row.mobSpecies}</span>
				{:else if column.key === 'estMarkupPct'}
					{#if row.estMarkupPct === null}
						<span class="text-text-tertiary" title="No markup observations for this loot yet">
							no data
						</span>
					{:else}
						<span class="tabular-nums">{row.estMarkupPct.toFixed(2)}%</span>
					{/if}
				{:else if column.key === 'coveredTt'}
					<span
						class="tabular-nums text-text-secondary"
						title={`${row.coveredItemCount} of ${row.itemCount} loot items have observations`}
					>
						{coveragePct(row)}%
					</span>
				{:else if column.key === 'lootTt'}
					<span class="tabular-nums text-text-secondary">{formatSalesPed(row.lootTt)}</span>
				{/if}
			{/snippet}
		</DataTable>
	</Card>
{/if}
