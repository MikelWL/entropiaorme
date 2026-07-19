<script lang="ts">
	import { onMount } from 'svelte';
	import { listen } from '@tauri-apps/api/event';
	import { getCurrentWindow } from '@tauri-apps/api/window';
	import {
		endNavigation,
		getNavigationSnapshot,
		hideNavigationOverlays,
		replanNavigation,
		skipNavigationStop,
		toggleNavigationPause,
		undoNavigationStop,
		updateNavigationPosition,
		type NavigationRun,
	} from '$lib/api';
	import { createWindowSizeSync } from '$lib/windows/windowSize';
	import { formatGamePoint } from '$lib/features/maps/coords';
	import { pinGlyph } from '$lib/features/maps/pinIcons';

	let root: HTMLDivElement;
	let run = $state<NavigationRun | null>(null);
	let busy = $state(false);
	let feedback = $state<string | null>(null);
	const sizeSync = createWindowSizeSync(() => root);
	const active = $derived(run?.stops.find((stop) => stop.status === 'active') ?? null);
	const following = $derived(run?.stops.find((stop) => stop.status === 'pending') ?? null);
	const completed = $derived(run?.stops.filter((stop) => stop.status === 'visited' || stop.status === 'skipped').length ?? 0);
	const remainingDistance = $derived.by(() => {
		if (!run) return 0;
		let lon = run.currentLon;
		let lat = run.currentLat;
		let total = 0;
		for (const stop of run.stops.filter((candidate) => candidate.status === 'active' || candidate.status === 'pending')) {
			total += Math.hypot(stop.lon - lon, stop.lat - lat);
			lon = stop.lon;
			lat = stop.lat;
		}
		return total;
	});

	async function hydrate() {
		try {
			run = await getNavigationSnapshot();
			if (!run) void hideNavigationOverlays();
		} catch {
			feedback = 'Navigation is unavailable.';
		}
	}

	onMount(() => {
		let unlisten: (() => void) | undefined;
		void hydrate();
		void listen('navigation:updated', hydrate).then((stop) => (unlisten = stop));
		sizeSync.schedule();
		const observer = new ResizeObserver(() => sizeSync.schedule());
		observer.observe(root);
		return () => { unlisten?.(); observer.disconnect(); sizeSync.cancel(); };
	});

	async function act(action: () => Promise<NavigationRun>) {
		if (busy) return;
		busy = true;
		feedback = null;
		try { run = await action(); } catch { feedback = 'The route could not be updated.'; }
		finally { busy = false; }
	}

	async function updatePosition() {
		if (busy) return;
		busy = true;
		feedback = null;
		try {
			const result = await updateNavigationPosition();
			if (result.run) run = result.run;
			feedback = result.status === 'updated' ? 'Position updated.' : result.status === 'ambiguous' ? 'Several route points are within range.' : result.status === 'noRegion' ? 'Calibrate coordinate capture first.' : result.status === 'paused' ? 'Resume the route before updating.' : 'The position could not be read.';
		} catch { feedback = 'The position could not be read.'; }
		finally { busy = false; }
	}

	async function endRoute() {
		if (busy) return;
		busy = true;
		try { await endNavigation(); await hideNavigationOverlays(); } finally { busy = false; }
	}

	function drag(event: PointerEvent) {
		if (event.button !== 0 || !(event.target instanceof Element) || event.target.closest('button')) return;
		void getCurrentWindow().startDragging();
	}
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div bind:this={root} class="p-2" onpointerdown={drag}>
	<div class="glass-panel w-72 rounded-xl p-3 text-white shadow-xl">
		<div class="flex items-start justify-between gap-3 border-b border-white/10 pb-2">
			<div class="min-w-0">
				<p class="text-[9px] font-bold uppercase tracking-wider text-white/35">Route guidance</p>
				<p class="mt-1 truncate text-[11px] text-white/65">{run ? `${run.planet} · ${run.mapViewName ?? 'Default'} · ${run.hotkey.toUpperCase()} updates` : 'No route'}</p>
			</div>
			<button class="release-btn" aria-label="Hide route guidance" onclick={() => hideNavigationOverlays()}>×</button>
		</div>
		{#if run && active}
			<div class="py-3">
				<div class="flex items-center gap-2">
					<span class="text-xl">{pinGlyph(active.icon)}</span>
					<div class="min-w-0 flex-1">
						<p class="truncate text-sm font-semibold">{active.name}</p>
						<p class="text-[11px] tabular-nums text-white/55">{formatGamePoint(active)}</p>
					</div>
					<span class="text-[10px] tabular-nums text-sky-300">{run.distanceToActive?.toFixed(1) ?? '—'} m</span>
				</div>
				<div class="mt-2 flex justify-between text-[10px] text-white/45">
					<span>{completed + 1} / {run.stops.length}</span>
					<span>{run.bearingDegrees?.toFixed(0) ?? '—'}° · {remainingDistance.toFixed(0)} m left</span>
				</div>
				<p class="mt-1 text-[9px] text-white/35">Last position: {run.lastPositionAt == null ? 'route start' : new Date(run.lastPositionAt * 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' })}</p>
				{#if following}<p class="mt-2 truncate text-[10px] text-white/45">Next: {following.name} · {formatGamePoint(following)}</p>{/if}
			</div>
			<div class="grid grid-cols-3 gap-1.5">
				<button class="hud-btn primary" disabled={busy} onclick={updatePosition}>Update</button>
				<button class="hud-btn" disabled={busy} onclick={() => act(skipNavigationStop)}>Skip</button>
				<button class="hud-btn" disabled={busy} onclick={() => act(undoNavigationStop)}>Undo</button>
				<button class="hud-btn" disabled={busy} onclick={() => act(toggleNavigationPause)}>{run.status === 'paused' ? 'Resume' : 'Pause'}</button>
				<button class="hud-btn" disabled={busy} onclick={() => act(replanNavigation)}>Replan</button>
				<button class="hud-btn danger" disabled={busy} onclick={endRoute}>End</button>
			</div>
		{:else if run?.status === 'completed'}
			<div class="py-4 text-center">
				<p class="text-sm font-semibold text-emerald-300">Route complete</p>
				<p class="mt-1 text-[10px] text-white/45">{run.stops.length} stops resolved</p>
			</div>
			<div class="grid grid-cols-2 gap-1.5">
				<button class="hud-btn" disabled={busy} onclick={() => act(undoNavigationStop)}>Undo last</button>
				<button class="hud-btn primary" disabled={busy} onclick={endRoute}>Done</button>
			</div>
		{:else}
			<p class="py-4 text-xs text-white/55">No active route.</p>
		{/if}
		<div class="h-4 pt-1"><p class="truncate text-[9px] text-orange-300/85" role="status">{feedback ?? ''}</p></div>
	</div>
</div>

<style>
	.glass-panel { background: rgba(10,14,23,.88); backdrop-filter: blur(16px) saturate(150%); border: 1px solid rgba(255,255,255,.08); }
	.release-btn { color: rgba(255,255,255,.4); font-size: 18px; line-height: 1; cursor: pointer; }
	.hud-btn { border: 1px solid rgba(255,255,255,.1); background: rgba(255,255,255,.05); border-radius: 5px; padding: 5px 6px; color: rgba(255,255,255,.7); font-size: 10px; cursor: pointer; }
	.hud-btn:hover { border-color: rgba(56,189,248,.35); color: rgb(125 211 252); }
	.hud-btn.primary { background: rgba(56,189,248,.16); color: rgb(125 211 252); }
	.hud-btn.danger { color: rgb(253 186 116); }
	.hud-btn:disabled { opacity: .4; cursor: default; }
</style>
