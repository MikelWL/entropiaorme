<script lang="ts">
	import Card from '$lib/components/Card.svelte';
	import ErrorNotice from '$lib/components/ErrorNotice.svelte';
	import InfoTip from '$lib/components/InfoTip.svelte';
	import SegmentedControl from '$lib/components/SegmentedControl.svelte';
	import ConvertStockModal from '$lib/features/analytics/ConvertStockModal.svelte';
	import HuntingPrimaryView from '$lib/features/analytics/HuntingPrimaryView.svelte';
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

	const MODE_OPTIONS: { id: ConfidenceMode; label: string }[] = [
		{ id: 'liquid', label: 'High Vol. Only' },
		{ id: 'liquidMiddling', label: 'High & Mid Vol.' },
		{ id: 'all', label: 'High, Mid & Low Vol.' },
	];
</script>

{#if model.loading}
	<p class="text-sm text-text-secondary">Loading hunting data...</p>
{:else if model.error && !model.overall}
	<ErrorNotice message={model.error} />
{:else if model.overall}
	<div class="space-y-5" data-guide-anchor="analytics-hunting-area">
		<ErrorNotice message={model.error} />

		<div class="flex flex-wrap items-center justify-between gap-3">
			<SegmentedControl
				options={ANALYTICS_RANGES.map((range) => ({ id: range, label: range }))}
				active={model.activeRange}
				onchange={(id) => (model.activeRange = id)}
			/>

			<div class="flex items-center gap-2.5">
				<span class="eyebrow">Markup confidence</span>
				<InfoTip label="How markup confidence works">
					<div class="space-y-2 text-xs leading-relaxed text-text-secondary">
						<p class="font-semibold text-text">
							Markup confidence: Choose which market prices to use
						</p>
						<p>
							Each level uses the item's markup, how much TT value has sold, how recent those
							sales are, and whether the markup can cover the auction fee.
						</p>
						<ul class="space-y-1.5">
							<li>
								<span class="text-text font-medium">High Vol.</span> Enough TT value sells each
								week to make the markup practical to realise.
							</li>
							<li>
								<span class="text-text font-medium">Mid Vol.</span> Sales are less frequent, but the
								markup is high enough for a practical sale to cover the 0.5 PED minimum fee.
							</li>
							<li>
								<span class="text-text font-medium">Low Vol.</span> Too little TT value has sold
								recently to rely on the markup.
							</li>
						</ul>
						<p>
							Excluded items use the Nanocube markup instead. The amount you currently hold does
							not affect these levels.
						</p>
					</div>
				</InfoTip>
				<SegmentedControl
					options={MODE_OPTIONS}
					active={model.confidenceMode}
					onchange={(id) => (model.confidenceMode = id as ConfidenceMode)}
				/>
			</div>
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
		oncancel={() => (sellItem = null)}
		activityAttributionNoun="a hunted species"
	/>
	<ConvertStockModal
		item={convertItem}
		onconvert={model.recycleStock}
		oncancel={() => (convertItem = null)}
	/>
{:else}
	<Card class="p-6">
		<p class="text-sm text-text-tertiary text-center" data-guide-anchor="analytics-hunting-area">
			No hunting data yet. Track a hunting session to compare your routines and activities.
		</p>
	</Card>
{/if}
