<script lang="ts">
	import type { TableModel } from '$lib/view/tableModel.svelte';
	import HuntingSessionPicker from './HuntingSessionPicker.svelte';
	import HuntingSessions from './HuntingSessions.svelte';
	import TreeCuttingStats from './TreeCuttingStats.svelte';
	import type {
		HuntingOverallLine,
		HuntingSessionSection,
	} from './huntingModel.svelte';

	let {
		overall,
		table,
		selected,
		totalCount,
		onselect,
	}: {
		overall: HuntingOverallLine;
		table: TableModel<HuntingSessionSection>;
		selected: HuntingSessionSection | null;
		totalCount: number;
		onselect: (key: string | null) => void;
	} = $props();

	const line = $derived(selected ?? overall);
</script>

{#snippet scopeControl()}
	<HuntingSessionPicker {table} {selected} {overall} {totalCount} {onselect} />
{/snippet}

<section class="relative" data-testid="hunting-primary-surface">
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
			lootMarkupFactor={line.lootMarkupFactor}
			expectedTtRate={line.expectedTtRate}
			expectedMarketRate={line.expectedMarketRate}
			expectedEconomics={line.expected}
			realisedReturns={line.realisedReturns}
			realisedRate={line.realisedRate}
			headingControl={scopeControl}
		/>

		{#if selected}
			<div class="mt-5">
				<HuntingSessions {selected} />
			</div>
		{/if}
	{/if}
</section>
