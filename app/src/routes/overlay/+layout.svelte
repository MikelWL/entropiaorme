<script lang="ts">
	import { onMount } from 'svelte';
	import { listen } from '@tauri-apps/api/event';
	import {
		initStatsCustomisation,
		overlayStats,
		OVERLAY_STATS_CHANGED_EVENT,
		type StatPref,
	} from '$lib/statsCustomisation.svelte';
	import {
		initStatsScope,
		statsScope,
		STATS_SCOPE_CHANGED_EVENT,
		type StatsScope
	} from '$lib/statsScope.svelte';

	let { children } = $props();

	onMount(() => {
		void initStatsCustomisation();
		void initStatsScope();
		let unlisten: (() => void) | undefined;
		let unlistenScope: (() => void) | undefined;
		void (async () => {
			unlisten = await listen<StatPref[]>(OVERLAY_STATS_CHANGED_EVENT, (event) => {
				if (Array.isArray(event.payload)) overlayStats.current = event.payload;
			});
		})();
		// The scope is shared, so a flip on the dashboard moves the
		// overlay's figures with it (and the other way round).
		void (async () => {
			unlistenScope = await listen<StatsScope>(STATS_SCOPE_CHANGED_EVENT, (event) => {
				if (event.payload === 'instance' || event.payload === 'lifetime') {
					statsScope.current = event.payload;
				}
			});
		})();
		return () => {
			unlisten?.();
			unlistenScope?.();
		};
	});
</script>

<div class="overlay-root select-none">
	{@render children()}
</div>

<style>
	:global(html),
	:global(body) {
		background: transparent !important;
		overflow: clip !important;
		margin: 0 !important;
		padding: 0 !important;
		width: max-content !important;
		height: max-content !important;
		min-width: 0 !important;
		min-height: 0 !important;
	}

	.overlay-root {
		background: transparent;
		overflow: visible;
		width: max-content;
		height: max-content;
	}
</style>
