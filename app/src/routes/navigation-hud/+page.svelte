<script lang="ts">
	import { onMount } from 'svelte';
	import { listen } from '@tauri-apps/api/event';
	import { getCurrentWindow } from '@tauri-apps/api/window';
	import {
		endNavigation,
		getNavigationSnapshot,
		hideNavigationOverlays,
		markNavigationVisited,
		resolveNavigationHarvest,
		scanMapCoordinates,
		skipNavigationStop,
		startNavigation,
		undoNavigationStop,
		updateNavigationPosition,
		type NavigationPositionStatus,
		type NavigationRun,
	} from '$lib/api';
	import { createWindowSizeSync } from '$lib/windows/windowSize';
	import { formatGamePoint, type GamePoint } from '$lib/features/maps/coords';
	import { pinGlyph } from '$lib/features/maps/pinIcons';
	import {
		acceptCartographyContextBroadcast,
		cartographyScanFailureMessage,
		CARTOGRAPHY_OVERLAY_CHANGED_EVENT,
	} from '$lib/features/maps/cartographyOverlay.svelte';
	import { describeError } from '$lib/view/errorState';

	const NAVIGATION_HOTKEYS = ['f6', 'f7', 'f8', 'f9', 'f10', 'f11', 'f12'];

	let root: HTMLDivElement;
	let run = $state<NavigationRun | null>(null);
	let busy = $state(false);
	let feedback = $state<string | null>(null);
	// Route setup lives here (not on the Maps page) so a single-monitor player
	// can plan while the game is fullscreen. Planet/map context arrives from the
	// main surface over the shared cartography-context broadcast.
	let planet = $state<string | null>(null);
	let mapViewId = $state<number | null>(null);
	let start = $state<GamePoint | null>(null);
	// Absent stop count charts every available pin; an explicit count stays capped.
	let hops = $state<number | null>(null);
	let hotkey = $state('f8');
	const canStart = $derived(
		!busy &&
			start != null &&
			planet != null &&
			(hops == null || (hops >= 1 && hops <= 500)),
	);
	// Set when a manual Visited lands outside the arrival tolerance: the visit
	// is held until the user confirms a forced record.
	let pendingVisit = $state<{ name: string; distance: number } | null>(null);
	const sizeSync = createWindowSizeSync(() => root);
	const active = $derived(run?.stops.find((stop) => stop.status === 'active') ?? null);
	const following = $derived(run?.stops.find((stop) => stop.status === 'pending') ?? null);
	// A harvest was detected beyond the arrival radius; the overlay asks whether
	// it was the intended tree rather than dropping it.
	const pendingHarvest = $derived(run?.pendingHarvest ?? null);
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

	function statusFeedback(status: NavigationPositionStatus): string {
		switch (status) {
			case 'updated':
				return 'Position updated.';
			case 'noActiveRun':
				return 'No active route.';
			case 'noRegion':
				return 'Calibrate coordinate capture first.';
			case 'ambiguous':
				return 'Several route points are within range.';
			case 'unreadable':
				return 'The coordinates could not be read.';
			case 'implausible':
				return 'That reading looked implausible.';
			default:
				return 'The position could not be read.';
		}
	}

	// Automatic harvesting advances the route from the tracker. Diffing the
	// previous snapshot against the next surfaces which tree was recorded, and
	// whether it was reached out of order (so the remaining path was recomputed).
	function applyHarvestFeedback(prev: NavigationRun, next: NavigationRun) {
		const before = new Map(prev.stops.map((stop) => [stop.id, stop.status]));
		const prevActiveId = prev.stops.find((stop) => stop.status === 'active')?.id;
		for (const stop of next.stops) {
			const priorStatus = before.get(stop.id);
			if (
				stop.status === 'visited' &&
				priorStatus != null &&
				priorStatus !== 'visited' &&
				stop.completionSource === 'harvest'
			) {
				feedback =
					priorStatus === 'active' || stop.id === prevActiveId
						? `Recorded ${stop.name}.`
						: `${stop.name} visited out of order; route recomputed.`;
				pendingVisit = null;
			}
		}
	}

	async function hydrate() {
		try {
			const next = await getNavigationSnapshot();
			if (next && run) applyHarvestFeedback(run, next);
			run = next;
			// No run means the setup panel is shown; the overlay only hides on an
			// explicit close, not whenever a route ends.
		} catch {
			feedback = 'Navigation is unavailable.';
		}
	}

	onMount(() => {
		let unlisten: (() => void) | undefined;
		let unlistenContext: (() => void) | undefined;
		void hydrate();
		void listen('navigation:updated', hydrate).then((stop) => (unlisten = stop));
		void listen(CARTOGRAPHY_OVERLAY_CHANGED_EVENT, (event) => {
			const context = acceptCartographyContextBroadcast(event.payload);
			planet = context.planet;
			mapViewId = context.mapViewId;
		}).then((stop) => (unlistenContext = stop));
		sizeSync.schedule();
		const observer = new ResizeObserver(() => sizeSync.schedule());
		observer.observe(root);
		return () => { unlisten?.(); unlistenContext?.(); observer.disconnect(); sizeSync.cancel(); };
	});

	// Capture the current in-game coordinates as the route start. A reading that
	// OCRs cleanly but falls outside the planet's map bounds ('implausible') still
	// carries usable numbers, so it seeds the route with a note rather than being
	// discarded as a failure.
	async function captureStart() {
		if (busy || !planet) return;
		busy = true;
		feedback = null;
		try {
			const result = await scanMapCoordinates(planet);
			if (
				(result.status === 'read' || result.status === 'implausible') &&
				result.lon != null &&
				result.lat != null
			) {
				start = { lon: result.lon, lat: result.lat };
				feedback =
					result.status === 'implausible'
						? `Captured ${formatGamePoint(start)} (reads outside ${planet}).`
						: null;
			} else {
				feedback = cartographyScanFailureMessage(result.status, planet);
			}
		} catch (cause) {
			feedback = describeError(cause, 'The current position could not be captured');
		} finally {
			busy = false;
		}
	}

	async function beginRoute() {
		if (!canStart || !start || !planet) return;
		busy = true;
		feedback = null;
		try {
			run = await startNavigation(planet, mapViewId, start.lon, start.lat, hops, hotkey);
			start = null;
			// The main surface repositions the HUD and radar around the live route.
		} catch (cause) {
			feedback = describeError(cause, 'The route could not be created');
		} finally {
			busy = false;
		}
	}

	async function closeOverlay() {
		if (busy) return;
		busy = true;
		try { await hideNavigationOverlays(); } finally { busy = false; }
	}

	async function act(action: () => Promise<NavigationRun>) {
		if (busy) return;
		busy = true;
		feedback = null;
		pendingVisit = null;
		try { run = await action(); } catch { feedback = 'The route could not be updated.'; }
		finally { busy = false; }
	}

	// Update strictly observes: it refreshes the distance and bearing to the
	// active tree without ever recording a visit.
	async function updatePosition() {
		if (busy) return;
		busy = true;
		feedback = null;
		pendingVisit = null;
		try {
			const result = await updateNavigationPosition();
			if (result.run) run = result.run;
			feedback = statusFeedback(result.status);
		} catch { feedback = 'The position could not be read.'; }
		finally { busy = false; }
	}

	// Visited records the active tree. Outside the arrival tolerance the visit
	// is held for an explicit confirmation (force).
	async function markVisited(force: boolean) {
		if (busy) return;
		busy = true;
		feedback = null;
		const target = active?.name ?? 'this tree';
		try {
			const result = await markNavigationVisited(force);
			if (result.run) run = result.run;
			if (result.status === 'updated') {
				pendingVisit = null;
				feedback = `Recorded ${target}.`;
			} else if (result.status === 'outOfTolerance') {
				pendingVisit = { name: target, distance: result.run?.distanceToActive ?? 0 };
			} else {
				pendingVisit = null;
				feedback = statusFeedback(result.status);
			}
		} catch { feedback = 'The visit could not be recorded.'; }
		finally { busy = false; }
	}

	// A harvest landed beyond the arrival radius. EU trees cut from far away, so
	// the overlay asks whether this was the intended tree; confirm records it,
	// dismiss leaves the route untouched.
	async function resolveHarvest(confirm: boolean) {
		if (busy) return;
		busy = true;
		feedback = null;
		pendingVisit = null;
		try {
			run = await resolveNavigationHarvest(confirm);
			feedback = confirm ? 'Recorded harvest.' : 'Harvest dismissed.';
		} catch {
			feedback = 'The harvest could not be updated.';
		} finally {
			busy = false;
		}
	}

	// Closing the HUD ends the visible navigation interaction; starting a new
	// route is the way to replan.
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
				<p class="text-[9px] font-bold uppercase tracking-wider text-white/35">{run ? 'Route guidance' : 'Plan route'}</p>
				<p class="mt-1 truncate text-[11px] text-white/65">{run ? `${run.planet} · ${run.mapViewName ?? 'Default'} · ${run.hotkey.toUpperCase()} updates` : planet ? `${planet} · new route` : 'No planet selected'}</p>
			</div>
			{#if run}
				<button class="release-btn" aria-label="End route and close" onclick={endRoute}>×</button>
			{:else}
				<button class="release-btn" aria-label="Close" onclick={closeOverlay}>×</button>
			{/if}
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
			{#if pendingHarvest}
				<div class="mb-2 rounded-md border border-emerald-300/30 bg-emerald-300/10 p-2">
					<p class="text-[10px] text-emerald-200">Harvest {pendingHarvest.distance.toFixed(1)} m from {pendingHarvest.name}. Cutting this tree?</p>
					<div class="mt-1.5 grid grid-cols-2 gap-1.5">
						<button class="hud-btn primary" disabled={busy} onclick={() => resolveHarvest(true)}>Yes, mark cut</button>
						<button class="hud-btn" disabled={busy} onclick={() => resolveHarvest(false)}>Not this one</button>
					</div>
				</div>
			{/if}
			{#if pendingVisit}
				<div class="mb-2 rounded-md border border-orange-300/30 bg-orange-300/10 p-2">
					<p class="text-[10px] text-orange-200">{pendingVisit.distance.toFixed(1)} m from {pendingVisit.name}. Mark it visited anyway?</p>
					<div class="mt-1.5 grid grid-cols-2 gap-1.5">
						<button class="hud-btn primary" disabled={busy} onclick={() => markVisited(true)}>Visit anyway</button>
						<button class="hud-btn" disabled={busy} onclick={() => (pendingVisit = null)}>Cancel</button>
					</div>
				</div>
			{/if}
			<div class="grid grid-cols-2 gap-1.5">
				<button class="hud-btn primary" disabled={busy} onclick={updatePosition}>Update</button>
				<button class="hud-btn" disabled={busy} onclick={() => markVisited(false)}>Visited</button>
				<button class="hud-btn" disabled={busy} onclick={() => act(skipNavigationStop)}>Skip</button>
				<button class="hud-btn" disabled={busy} onclick={() => act(undoNavigationStop)}>Undo</button>
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
		{:else if planet}
			<div class="space-y-2.5 py-3">
				<div>
					<p class="text-[9px] uppercase tracking-wider text-white/35">Starting position</p>
					<p class="mt-0.5 text-[11px] tabular-nums text-white/70">{start ? formatGamePoint(start) : 'Not captured'}</p>
				</div>
				<button class="hud-btn primary w-full" disabled={busy} onclick={captureStart}>
					{start ? 'Capture again' : 'Capture current position'}
				</button>
				<label class="block">
					<span class="mb-0.5 block text-[9px] uppercase tracking-wider text-white/35">Stops (blank = all pins)</span>
					<input class="hud-field w-full" type="number" min="1" max="500" placeholder="All pins" bind:value={hops} />
				</label>
				<label class="block">
					<span class="mb-0.5 block text-[9px] uppercase tracking-wider text-white/35">Update hotkey</span>
					<select class="hud-field w-full" bind:value={hotkey} aria-label="Navigation update hotkey">
						{#each NAVIGATION_HOTKEYS as key}<option value={key}>{key.toUpperCase()}</option>{/each}
					</select>
				</label>
				<button class="hud-btn primary w-full" disabled={!canStart} onclick={beginRoute}>Start route</button>
			</div>
		{:else}
			<p class="py-4 text-xs text-white/55">Open a planet map in the app to plan a route.</p>
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
	.hud-btn:disabled { opacity: .4; cursor: default; }
	.hud-field { width: 100%; border: 1px solid rgba(255,255,255,.12); background: rgba(255,255,255,.05); border-radius: 5px; padding: 4px 7px; color: rgba(255,255,255,.85); font-size: 11px; }
	.hud-field:focus { outline: none; border-color: rgba(56,189,248,.5); }
	select.hud-field {
		appearance: none;
		padding-right: 22px;
		cursor: pointer;
		background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 12 12' fill='none' stroke='%23ffffff88' stroke-width='1.4'%3E%3Cpath d='M3 4.5 6 7.5 9 4.5'/%3E%3C/svg%3E");
		background-repeat: no-repeat;
		background-position: right 6px center;
		background-size: 12px;
	}
	.hud-field option { color: #0a0e17; background: #e5edf5; }
</style>
