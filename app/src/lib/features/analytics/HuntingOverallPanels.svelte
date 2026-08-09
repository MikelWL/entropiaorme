<script lang="ts">
	import SegmentedControl from '$lib/components/SegmentedControl.svelte';
	import type { ActivityHistoryEntry, AuctionListing } from '$lib/types/analytics';
	import ActivityHistory from './ActivityHistory.svelte';
	import AuctionListings from './AuctionListings.svelte';
	import TreeCuttingStock from './TreeCuttingStock.svelte';
	import type { TreeCuttingStock as StockRow } from './treeCuttingModel.svelte';

	export type HuntingOverallPanel = 'stock' | 'market' | 'history';

	let {
		active,
		stock,
		openListings,
		resolvedListings,
		history,
		historyLoading,
		onchange,
		onsell,
		onconvert,
		onresolve,
		onundo,
	}: {
		active: HuntingOverallPanel;
		stock: StockRow[];
		openListings: AuctionListing[];
		resolvedListings: AuctionListing[];
		history: ActivityHistoryEntry[];
		historyLoading: boolean;
		onchange: (panel: HuntingOverallPanel) => void;
		onsell: (item: StockRow) => void;
		onconvert: (item: StockRow) => void;
		onresolve: (listingId: string, outcome: { sold: true; finalPrice: number; saleFee: number; resolvedAt?: string } | { sold: false; resolvedAt?: string }) => Promise<void>;
		onundo: (entry: ActivityHistoryEntry, revertSale?: boolean) => Promise<void>;
	} = $props();

	const PANELS = [
		{ id: 'stock', label: 'Stock' },
		{ id: 'market', label: 'Market' },
		{ id: 'history', label: 'History' },
	];
</script>

<div class="border-t border-border/50 pt-5">
	<SegmentedControl
		options={PANELS}
		{active}
		onchange={(id) => onchange(id as HuntingOverallPanel)}
	/>

	<div class="mt-4">
		{#if active === 'stock'}
			{#if stock.length > 0}
				<TreeCuttingStock
					{stock}
					onsell={onsell}
					onconvert={onconvert}
					sourceDescription="Loot recorded from hunting, minus loot you have already sold or converted."
				/>
			{:else}
				<p class="py-8 text-center text-sm text-text-tertiary">No hunted stock recorded.</p>
			{/if}
		{:else if active === 'market'}
			<AuctionListings
				open={openListings}
				resolved={resolvedListings}
				onresolve={onresolve}
				activityNoun="hunting"
				sourceNounPlural="the species"
				sourceNounIndefinite="a species"
				emptyLead="Selling hunted stock"
				expiredChargeNote="No species is charged for it: not selling describes the market and the price you asked, not the hunting that produced the stock."
			/>
		{:else}
			<ActivityHistory entries={history} loading={historyLoading} onundo={onundo} />
		{/if}
	</div>
</div>
