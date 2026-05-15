<script lang="ts">
	import { onMount } from 'svelte';
	import { listen } from '@tauri-apps/api/event';
	import {
		initStatsCustomisation,
		overlayStats,
		OVERLAY_STATS_CHANGED_EVENT,
		type StatPref,
	} from '$lib/statsCustomisation';

	let { children } = $props();

	onMount(() => {
		void initStatsCustomisation();
		let unlisten: (() => void) | undefined;
		void (async () => {
			unlisten = await listen<StatPref[]>(OVERLAY_STATS_CHANGED_EVENT, (event) => {
				if (Array.isArray(event.payload)) overlayStats.set(event.payload);
			});
		})();
		return () => {
			unlisten?.();
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
