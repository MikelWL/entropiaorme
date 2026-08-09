<script lang="ts">
	import Card from '$lib/components/Card.svelte';
	import type { ActivityHistoryEntry, AuctionListing } from '$lib/types/analytics';
	import type { TableModel } from '$lib/view/tableModel.svelte';
	import HuntingOverallPanels, { type HuntingOverallPanel } from './HuntingOverallPanels.svelte';
	import HuntingSessionPicker from './HuntingSessionPicker.svelte';
	import HuntingSessions from './HuntingSessions.svelte';
	import TreeCuttingStats from './TreeCuttingStats.svelte';
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
		overallPanel,
		onpanelchange,
		openListings,
		resolvedListings,
		history,
		historyLoading,
		onresolve,
		onundo,
	}: {
		overall: HuntingOverallLine;
		stock: StockRow[];
		table: TableModel<HuntingSessionSection>;
		selected: HuntingSessionSection | null;
		totalCount: number;
		onselect: (key: string | null) => void;
		onsell: (item: StockRow) => void;
		onconvert: (item: StockRow) => void;
		overallPanel: HuntingOverallPanel;
		onpanelchange: (panel: HuntingOverallPanel) => void;
		openListings: AuctionListing[];
		resolvedListings: AuctionListing[];
		history: ActivityHistoryEntry[];
		historyLoading: boolean;
		onresolve: (listingId: string, outcome: { sold: true; finalPrice: number; saleFee: number; resolvedAt?: string } | { sold: false; resolvedAt?: string }) => Promise<void>;
		onundo: (entry: ActivityHistoryEntry, revertSale?: boolean) => Promise<void>;
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
			{:else}
				<HuntingOverallPanels
					active={overallPanel}
					{stock}
					{openListings}
					{resolvedListings}
					{history}
					{historyLoading}
					onchange={onpanelchange}
					{onsell}
					{onconvert}
					{onresolve}
					{onundo}
				/>
			{/if}
		</div>
	{/if}
</Card>
