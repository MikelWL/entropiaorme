<script lang="ts">
	import Card from '$lib/components/Card.svelte';
	import DataTable from '$lib/components/DataTable.svelte';
	import ErrorNotice from '$lib/components/ErrorNotice.svelte';
	import type { MarketWeaponBreakEven } from '$lib/api';
	import { createBreakEvenModel } from '$lib/features/market/breakEvenModel.svelte';

	const model = createBreakEvenModel();

	$effect(() => {
		void model.loadData();
	});

	const columns = $derived([
		{ key: 'name', label: 'Weapon' },
		{ key: 'efficiencyPct', label: 'Efficiency', align: 'right' as const },
		...model.looters.map((looter) => ({
			key: `looter:${looter.name}`,
			label: `${looter.name} ${looter.level.toFixed(0)}`,
			align: 'right' as const
		}))
	]);

	function cellFor(row: MarketWeaponBreakEven, key: string) {
		return row.cells.find((cell) => `looter:${cell.looterName}` === key) ?? null;
	}
</script>

{#if model.error}
	<ErrorNotice message={model.error} />
{:else if !model.loading && model.weapons.length === 0}
	<Card>
		<div class="py-10 text-center space-y-2">
			<p class="text-sm text-text-secondary">No weapons in your equipment library yet.</p>
			<p class="text-sm text-text-tertiary">
				Add the weapons you hunt with on the Equipment page to see their break-even markup.
			</p>
		</div>
	</Card>
{:else}
	<Card>
		<p class="text-sm text-text-secondary mb-4">
			The overall loot markup each weapon needs to break even, per looter profession. Modelled
			from weapon efficiency and looter level (roughly a one percentage point error bar); an
			estimate to steer by, never a measured rate.
		</p>
		<DataTable
			{columns}
			rows={model.weapons}
			emptyMessage={model.loading ? 'Loading break-even data' : 'No weapons'}
		>
			{#snippet cell({ row, column }: { row: MarketWeaponBreakEven; column: { key: string } })}
				{#if column.key === 'name'}
					<span class="text-text">{row.name}</span>
				{:else if column.key === 'efficiencyPct'}
					{#if row.efficiencyPct === null}
						<span class="text-text-tertiary" title="Not in the bundled item catalogue">
							unknown
						</span>
					{:else}
						<span class="tabular-nums text-text-secondary">{row.efficiencyPct.toFixed(1)}%</span>
					{/if}
				{:else}
					{@const c = cellFor(row, column.key)}
					{#if c === null}
						<span class="text-text-tertiary">-</span>
					{:else}
						<span
							class="tabular-nums"
							title={`Modelled TT return ${c.ttReturnPct.toFixed(1)}%`}
						>
							{c.breakEvenMarkupPct.toFixed(1)}%
						</span>
					{/if}
				{/if}
			{/snippet}
		</DataTable>
	</Card>
{/if}
