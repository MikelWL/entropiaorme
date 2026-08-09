<script lang="ts">
	import Card from '$lib/components/Card.svelte';
	import type { TableModel } from '$lib/view/tableModel.svelte';
	import HuntingSessionPicker from './HuntingSessionPicker.svelte';
	import HuntingSessions from './HuntingSessions.svelte';
	import TreeCuttingStats from './TreeCuttingStats.svelte';
	import TreeCuttingStock from './TreeCuttingStock.svelte';
	import type {
		HuntingOverallLine,
		HuntingSessionSection,
	} from './huntingModel.svelte';
	import type { TreeCuttingStock as StockRow } from './treeCuttingModel.svelte';

	let {
		overall,
		stock,
		table,
		selected,
		totalCount,
		onselect,
		onsell,
		onconvert,
	}: {
		overall: HuntingOverallLine;
		stock: StockRow[];
		table: TableModel<HuntingSessionSection>;
		selected: HuntingSessionSection | null;
		totalCount: number;
		onselect: (key: string | null) => void;
		onsell: (item: StockRow) => void;
		onconvert: (item: StockRow) => void;
	} = $props();

	const line = $derived(selected ?? overall);
</script>

{#snippet scopeControl()}
	<HuntingSessionPicker {table} {selected} {overall} {totalCount} {onselect} />
{/snippet}

<Card class="relative hover:z-20 border-accent/30 p-6 shadow-lg backdrop-blur-[2px] bg-gradient-to-br from-accent/[0.12] via-surface/70 to-surface/70">
	{#if selected?.isUnassigned}
		<div class="min-w-0">
			{@render scopeControl()}
			<div class="mt-5">
				<HuntingSessions {selected} />
			</div>
		</div>
	{:else}
		<TreeCuttingStats
			cycled={line.cycled}
			returns={line.returns}
			lootRate={line.lootRate}
			muProjectedReturns={line.muProjectedReturns}
			muRate={line.muRate}
			realisedReturns={line.realisedReturns}
			realisedRate={line.realisedRate}
			headingControl={scopeControl}
		/>

		<div class="mt-5">
			{#if selected}
				<HuntingSessions {selected} />
			{:else if stock.length > 0}
				<div class="border-t border-border/50 pt-5">
					<TreeCuttingStock
						{stock}
						onsell={onsell}
						onconvert={onconvert}
						sourceDescription="Loot recorded from hunting, minus loot you have already sold or converted."
					/>
				</div>
			{/if}
		</div>
	{/if}
</Card>
