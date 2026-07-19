<script lang="ts">
	import { onMount } from 'svelte';
	import { listen } from '@tauri-apps/api/event';
	import { getCurrentWindow } from '@tauri-apps/api/window';
	import { getNavigationSnapshot, getRadarGeometry, type NavigationRun } from '$lib/api';

	// The in-game radar edge is 150 game units (metres) from the player at its
	// centre, so a tree within that range has a true position on the radar, not
	// just a direction. EDGE is that range in the 0..100 SVG space, kept a hair
	// inside the window so an at-range dot stays fully visible.
	const RADAR_RANGE_M = 150;
	const EDGE = 47;

	let run = $state<NavigationRun | null>(null);
	const angle = $derived(((run?.bearingDegrees ?? 0) * Math.PI) / 180);
	const x2 = $derived(50 + Math.sin(angle) * EDGE);
	const y2 = $derived(50 - Math.cos(angle) * EDGE);
	// Within range the dot sits at the tree's scaled distance; beyond it, only
	// the bearing line points the way.
	const distance = $derived(run?.distanceToActive ?? null);
	const inRange = $derived(distance != null && distance <= RADAR_RANGE_M);
	const dotRadius = $derived(inRange && distance != null ? (distance / RADAR_RANGE_M) * EDGE : 0);
	const dotX = $derived(50 + Math.sin(angle) * dotRadius);
	const dotY = $derived(50 - Math.cos(angle) * dotRadius);
	// The line stops at the dot when the tree is in range; only a beyond-range
	// tree draws the full line to the radar edge.
	const lineX = $derived(inRange ? dotX : x2);
	const lineY = $derived(inRange ? dotY : y2);

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
		<line x1="50" y1="50" x2={lineX} y2={lineY} stroke="rgba(125,211,252,.9)" stroke-width="2" stroke-linecap="round" filter="url(#glow)" />
		{#if inRange}
			<circle cx={dotX} cy={dotY} r="3.5" fill="rgba(125,211,252,.95)" filter="url(#glow)" />
		{/if}
		<circle cx="50" cy="50" r="2" fill="rgba(255,255,255,.7)" />
	</svg>
{/if}

<style>
	:global(html), :global(body), :global(.overlay-root) { width: 100% !important; height: 100% !important; pointer-events: none !important; }
</style>
