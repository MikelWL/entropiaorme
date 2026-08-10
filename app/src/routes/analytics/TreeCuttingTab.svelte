<script lang="ts">
	import Card from '$lib/components/Card.svelte';
	import ErrorNotice from '$lib/components/ErrorNotice.svelte';
	import SegmentedControl from '$lib/components/SegmentedControl.svelte';
	import ActivityHistory from '$lib/features/analytics/ActivityHistory.svelte';
	import AdjustStockModal from '$lib/features/analytics/AdjustStockModal.svelte';
	import AuctionListings from '$lib/features/analytics/AuctionListings.svelte';
	import ConvertStockModal from '$lib/features/analytics/ConvertStockModal.svelte';
	import MarkupConfidenceControl from '$lib/features/analytics/MarkupConfidenceControl.svelte';
	import SellStockModal from '$lib/features/analytics/SellStockModal.svelte';
	import TreeCuttingActivities from '$lib/features/analytics/TreeCuttingActivities.svelte';
	import TreeCuttingStats from '$lib/features/analytics/TreeCuttingStats.svelte';
	import TreeCuttingStock from '$lib/features/analytics/TreeCuttingStock.svelte';
	import { ANALYTICS_RANGES } from '$lib/features/analytics/analyticsRange';
	import {
		createTreeCuttingModel,
		type ConfidenceMode,
		type TreeCuttingStock as StockRow,
	} from '$lib/features/analytics/treeCuttingModel.svelte';

	const model = createTreeCuttingModel();

	// The lower box answers three questions about the same output: how each
	// sub-activity performs, what is currently happening in the market with
	// what they produced, and what has already been done with it. Overall
	// stays put above all three, since the headline figures describe the
	// activity whichever is open.
	type ActivityView = 'activities' | 'market' | 'history';
	let activityView = $state<ActivityView>('activities');
	const ACTIVITY_VIEWS = [
		{ id: 'activities', label: 'Sub-activities' },
		{ id: 'market', label: 'Market' },
		{ id: 'history', label: 'History' },
	];

	let sellItem = $state<StockRow | null>(null);
	let convertItem = $state<StockRow | null>(null);
	let removeItem = $state<StockRow | null>(null);
	let shrapnelItem = $state<StockRow | null>(null);

	// History reads when it is opened rather than with the tab: an undo verdict
	// depends on every other entry, so it is worth computing fresh at the
	// moment it is offered.
	let historyLoading = $state(false);
	async function showView(id: ActivityView) {
		activityView = id;
		if (id !== 'history') return;
		historyLoading = true;
		try {
			await model.loadHistory();
		} catch {
			// The model records the failure and the tab shows it; the view
			// stays open on that notice rather than rejecting into nothing.
		} finally {
			historyLoading = false;
		}
	}

	$effect(() => {
		void model.loadData(model.period);
	});

</script>

{#if model.loading}
	<p class="text-sm text-text-secondary">Loading tree cutting data...</p>
{:else if model.error && !model.sections.length}
	<ErrorNotice message={model.error} />
{:else if model.sections.length > 0}
	<div class="space-y-5" data-guide-anchor="analytics-treecutting-area">
		<ErrorNotice message={model.error} />

		<div class="flex flex-wrap items-center justify-between gap-3">
			<SegmentedControl
				options={ANALYTICS_RANGES.map((range) => ({ id: range, label: range }))}
				active={model.activeRange}
				onchange={(id) => (model.activeRange = id)}
			/>

			<MarkupConfidenceControl
				active={model.confidenceMode}
				onchange={(id) => (model.confidenceMode = id)}
			/>
		</div>

		{#if model.overall}
			<div
				class="relative hover:z-20 rounded-xl border border-accent/30 p-6 shadow-lg
					backdrop-blur-[2px] bg-gradient-to-br from-accent/[0.12] via-surface/70 to-surface/70"
			>
				<div class="grid gap-x-8 gap-y-6 sm:grid-cols-[auto_minmax(0,1fr)]">
					<TreeCuttingStats
						heading="Overall"
						cycled={model.overall.cycled}
						returns={model.overall.returns}
						lootRate={model.overall.lootRate}
						muProjectedReturns={model.overall.muProjectedReturns}
						muRate={model.overall.muRate}
						realisedReturns={model.overall.realisedReturns}
						realisedRate={model.overall.realisedRate}
					/>

					{#if model.stock.length > 0}
						<TreeCuttingStock
							stock={model.stock}
							onsell={(item) => (sellItem = item)}
							onconvert={(item) => (convertItem = item)}
							onremove={(item) => (removeItem = item)}
							onshrapnelconvert={(item) => (shrapnelItem = item)}
						/>
					{/if}
				</div>
			</div>
		{/if}

		<div class="space-y-3">
			<SegmentedControl
				options={ACTIVITY_VIEWS}
				active={activityView}
				onchange={(id) => showView(id as ActivityView)}
			/>

			{#if activityView === 'activities'}
				<TreeCuttingActivities
					sections={model.activityTable.filtered}
					selected={model.selectedSection}
					onselect={(yieldTier) => model.selectSection(yieldTier)}
					sortKey={model.activityTable.sortKey}
					sortDir={model.activityTable.sortDir}
					onsort={(key) => model.activityTable.setSort(key)}
				/>
			{:else if activityView === 'market'}
				<AuctionListings
					open={model.openListings}
					resolved={model.resolvedListings}
					onresolve={model.resolveListing}
				/>
			{:else}
				<ActivityHistory
					entries={model.history}
					loading={historyLoading}
					onundo={model.undoHistoryEntry}
				/>
			{/if}
		</div>
	</div>

	<SellStockModal
		item={sellItem}
		onlist={model.listStock}
		ontrade={model.tradeStock}
		oncancel={() => (sellItem = null)}
	/>
	<ConvertStockModal
		item={convertItem}
		onconvert={model.recycleStock}
		oncancel={() => (convertItem = null)}
	/>
	<AdjustStockModal
		item={removeItem}
		mode="remove"
		onconfirm={model.discardStock}
		oncancel={() => (removeItem = null)}
	/>
	<AdjustStockModal
		item={shrapnelItem}
		mode="shrapnel"
		onconfirm={(_itemName, quantity) => model.convertShrapnelStock(quantity)}
		oncancel={() => (shrapnelItem = null)}
	/>
{:else}
	<Card class="p-6">
		<p class="text-sm text-text-tertiary text-center" data-guide-anchor="analytics-treecutting-area">
			No tree cutting data yet. Harvest trees during a tracked session to compare board outputs.
		</p>
	</Card>
{/if}
