<script lang="ts">
	import Tabs from '$lib/components/Tabs.svelte';
	import OverviewTab from './OverviewTab.svelte';
	import BreakEvenTab from './BreakEvenTab.svelte';
	import ImportTab from './ImportTab.svelte';
	import HistoryTab from './HistoryTab.svelte';

	const tabs = [
		{ id: 'overview', label: 'Overview' },
		{ id: 'break-even', label: 'Break-even' },
		{ id: 'import', label: 'Import' },
		{ id: 'history', label: 'History' }
	];

	let activeTab = $state('overview');
</script>

<div class="px-6 pb-6 space-y-6">
	<!-- Page header -->
	<div class="flex items-center justify-between">
		<header class="flex flex-col gap-1.5">
			<h1 class="text-xl font-semibold text-text tracking-tight">Market</h1>
			<span class="block h-px w-12 bg-gradient-to-r from-accent/60 to-transparent"></span>
			<p class="text-sm text-text-secondary mt-0.5">
				Loot markup observations from the in-game market ledger. Informational only: estimates
				never enter your P&amp;L.
			</p>
		</header>
	</div>

	<!-- Tab bar -->
	<Tabs {tabs} active={activeTab} onchange={(id) => (activeTab = id)} />

	<!-- Tab content. Tabs unmount when inactive, so Overview reloads on
	     re-entry (which is how a fresh import shows up after the Import
	     tab hands control back). -->
	<div>
		{#if activeTab === 'overview'}
			<OverviewTab />
		{:else if activeTab === 'break-even'}
			<BreakEvenTab />
		{:else if activeTab === 'import'}
			<ImportTab onimported={() => (activeTab = 'overview')} />
		{:else if activeTab === 'history'}
			<HistoryTab />
		{/if}
	</div>
</div>
