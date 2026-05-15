<script lang="ts">
	import { onMount } from 'svelte';
	import Tabs from '$lib/components/Tabs.svelte';
	import OverviewTab from './OverviewTab.svelte';
	import LedgerTab from './LedgerTab.svelte';
	import ActivityTab from './ActivityTab.svelte';
	import SessionsTab from './SessionsTab.svelte';
	import { getPreference } from '$lib/preferences';
	import { guideState, registerDemoApi, unregisterDemoApi } from '$lib/guide/state.svelte';
	import { closeGuide, openGuide } from '$lib/guide/engine';
	import { analyticsSurface } from '$lib/guide/surfaces/analytics';

	const tabs = [
		{ id: 'overview', label: 'Overview' },
		{ id: 'ledger', label: 'Ledger' },
		{ id: 'activity', label: 'Activity' },
		{ id: 'sessions', label: 'Sessions' }
	];

	let activeTab = $state('overview');

	// Guide
	let guideSeen = $state(true);
	function toggleSurfaceGuide(): void {
		if (guideState.isActive) {
			closeGuide();
		} else {
			guideSeen = true;
			void openGuide(analyticsSurface);
		}
	}

	onMount(() => {
		void (async () => {
			guideSeen = await getPreference<boolean>('guide_seen_analytics', false);
		})();
		registerDemoApi('analytics', {
			setTab: (tab: string) => {
				activeTab = tab;
			}
		});
		return () => unregisterDemoApi('analytics');
	});
</script>

<div class="px-6 pb-6 space-y-6">
	<!-- Page header -->
	<div class="flex items-center justify-between">
		<header class="flex flex-col gap-1.5">
			<h1 class="text-xl font-semibold text-text tracking-tight">Analytics</h1>
			<span class="block h-px w-12 bg-gradient-to-r from-accent/60 to-transparent"></span>
			<p class="text-sm text-text-secondary mt-0.5">Lifetime totals, ledger activity, and session history</p>
		</header>
		<div class="flex items-center gap-2">
			<button
				type="button"
				onclick={toggleSurfaceGuide}
				title={guideState.isActive ? 'Exit guide' : 'Open guide'}
				aria-label={guideState.isActive ? 'Exit guide' : 'Open guide for this page'}
				class="relative h-8 w-8 rounded-full border border-border bg-surface hover:bg-surface-hover text-text-secondary hover:text-text transition-colors flex items-center justify-center text-sm font-semibold {guideState.isActive ? 'z-[9100]' : ''}"
			>
				{#if guideState.isActive}
					<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="currentColor" class="w-3.5 h-3.5" aria-hidden="true">
						<path d="M5.28 4.22a.75.75 0 00-1.06 1.06L6.94 8l-2.72 2.72a.75.75 0 101.06 1.06L8 9.06l2.72 2.72a.75.75 0 101.06-1.06L9.06 8l2.72-2.72a.75.75 0 00-1.06-1.06L8 6.94 5.28 4.22z" />
					</svg>
				{:else}
					?
				{/if}
				{#if !guideSeen}
					<span class="absolute -top-0.5 -right-0.5 h-2 w-2 rounded-full bg-accent"></span>
				{/if}
			</button>
		</div>
	</div>

	<!-- Tab bar -->
	<Tabs {tabs} active={activeTab} onchange={(id) => (activeTab = id)} />

	<!-- Tab content (lazy-loaded). Keyed on guideState so all four tabs
	     re-mount when the user flips into or out of guide-mode; that re-runs
	     their data $effects against the swapped /demo/* endpoints. -->
	{#key guideState.isActive}
		<div>
			{#if activeTab === 'overview'}
				<OverviewTab />
			{:else if activeTab === 'ledger'}
				<LedgerTab />
			{:else if activeTab === 'activity'}
				<ActivityTab />
			{:else if activeTab === 'sessions'}
				<SessionsTab />
			{/if}
		</div>
	{/key}
</div>
