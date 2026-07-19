<script lang="ts">
	import { onMount } from 'svelte';
	import { listen } from '@tauri-apps/api/event';
	import { getCurrentWindow } from '@tauri-apps/api/window';
	import { getNavigationSnapshot, getRadarGeometry, type NavigationRun } from '$lib/api';

	let run = $state<NavigationRun | null>(null);
	const angle = $derived((run?.bearingDegrees ?? 0) * Math.PI / 180);
	const x2 = $derived(50 + Math.sin(angle) * 45);
	const y2 = $derived(50 - Math.cos(angle) * 45);

	async function hydrate() {
		const [nextRun, geometry] = await Promise.all([getNavigationSnapshot(), getRadarGeometry()]);
		run = nextRun;
		if (!nextRun || !geometry) void getCurrentWindow().hide();
	}

	onMount(() => {
		let unlisten: (() => void) | undefined;
		void hydrate().catch(() => getCurrentWindow().hide());
		void listen('navigation:updated', () => void hydrate()).then((stop) => (unlisten = stop));
		return () => unlisten?.();
	});
</script>

{#if run?.status === 'active' && run.bearingDegrees != null}
	<svg viewBox="0 0 100 100" class="block h-full w-full overflow-visible" aria-hidden="true">
		<defs>
			<filter id="glow"><feGaussianBlur stdDeviation="1.2" result="blur"/><feMerge><feMergeNode in="blur"/><feMergeNode in="SourceGraphic"/></feMerge></filter>
		</defs>
		<line x1="50" y1="50" {x2} {y2} stroke="rgba(125,211,252,.9)" stroke-width="2" stroke-linecap="round" filter="url(#glow)" />
		<circle cx={x2} cy={y2} r="3" fill="rgba(125,211,252,.95)" filter="url(#glow)" />
		<circle cx="50" cy="50" r="2" fill="rgba(255,255,255,.7)" />
	</svg>
{/if}

<style>
	:global(html), :global(body), :global(.overlay-root) { width: 100% !important; height: 100% !important; pointer-events: none !important; }
</style>
