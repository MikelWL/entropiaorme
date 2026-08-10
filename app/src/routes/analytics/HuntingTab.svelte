<script lang="ts">
	import ErrorNotice from '$lib/components/ErrorNotice.svelte';
	import SegmentedControl from '$lib/components/SegmentedControl.svelte';
	import AdjustStockModal from '$lib/features/analytics/AdjustStockModal.svelte';
	import ConvertStockModal from '$lib/features/analytics/ConvertStockModal.svelte';
	import HuntingPrimaryView from '$lib/features/analytics/HuntingPrimaryView.svelte';
	import MarkupConfidenceControl from '$lib/features/analytics/MarkupConfidenceControl.svelte';
	import type { HuntingOverallPanel } from '$lib/features/analytics/HuntingOverallPanels.svelte';
	import SellStockModal from '$lib/features/analytics/SellStockModal.svelte';
	import { ANALYTICS_RANGES } from '$lib/features/analytics/analyticsRange';
	import { createHuntingModel } from '$lib/features/analytics/huntingModel.svelte';
	import { registerDemoApi, unregisterDemoApi } from '$lib/guide/state.svelte';
	import { onMount } from 'svelte';
	import type {
		ConfidenceMode,
		TreeCuttingStock as StockRow,
	} from '$lib/features/analytics/treeCuttingModel.svelte';

	const model = createHuntingModel();

	let overallPanel = $state<HuntingOverallPanel>('stock');

	let sellItem = $state<StockRow | null>(null);
	let convertItem = $state<StockRow | null>(null);
	let removeItem = $state<StockRow | null>(null);
	let shrapnelItem = $state<StockRow | null>(null);

	// History reads when it is opened rather than with the tab: an undo verdict
	// depends on every other entry, so it is worth computing fresh at the
	// moment it is offered.
	let historyLoading = $state(false);
	async function showView(id: HuntingOverallPanel) {
		overallPanel = id;
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

	// The guide walks the four views it narrates rather than pointing at a
	// static frame (the LedgerTab precedent).
	onMount(() => {
		registerDemoApi('analytics-hunting', {
			setView: (view: string) => {
				if (view === 'overall' || view === 'sessions') {
					model.selectSession(null);
					void showView('stock');
					return;
				}
				model.selectSession(null);
				void showView(view as HuntingOverallPanel);
			},
		});
		return () => unregisterDemoApi('analytics-hunting');
	});

</script>

{#if model.loading}
	<p class="text-sm text-text-secondary">Loading hunting data...</p>
{:else if model.error && !model.overall}
	<ErrorNotice message={model.error} />
{:else if model.overall}
	<div class="space-y-5" data-guide-anchor="analytics-hunting-area">
		<ErrorNotice message={model.error} />

		<div class="flex flex-wrap items-center justify-between gap-3 pb-2">
			<SegmentedControl
				options={ANALYTICS_RANGES.map((range) => ({ id: range, label: range }))}
				active={model.activeRange}
				onchange={(id) => (model.activeRange = id)}
			/>

			<MarkupConfidenceControl
				active={model.confidenceMode}
				onchange={(id) => (model.confidenceMode = id as ConfidenceMode)}
			/>
		</div>

		<HuntingPrimaryView
			overall={model.overall}
			stock={model.stock}
			table={model.sessionTable}
			selected={model.selectedSession}
			totalCount={model.sessionSections.length}
			onselect={(key) => model.selectSession(key)}
			onsell={(item) => (sellItem = item)}
			onconvert={(item) => (convertItem = item)}
			onremove={(item) => (removeItem = item)}
			onshrapnelconvert={(item) => (shrapnelItem = item)}
			overallPanel={overallPanel}
			onpanelchange={showView}
			openListings={model.openListings}
			resolvedListings={model.resolvedListings}
			history={model.history}
			{historyLoading}
			onresolve={model.resolveListing}
			onundo={model.undoHistoryEntry}
		/>
	</div>

	<SellStockModal
		item={sellItem}
		onlist={model.listStock}
		ontrade={model.tradeStock}
		oncancel={() => (sellItem = null)}
		activityAttributionNoun="a hunted species"
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
	<ConvertStockModal
		item={convertItem}
		onconvert={model.recycleStock}
		oncancel={() => (convertItem = null)}
	/>
{:else}
	<p
		class="py-10 text-center text-sm text-text-tertiary"
		data-guide-anchor="analytics-hunting-area"
	>
		No hunting data yet. Track a hunting session to compare your routines and activities.
	</p>
{/if}
