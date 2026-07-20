/**
 * The cartography pin overlay's controller: the planet/map-view lists the strip
 * reads and the lifecycle wiring that keeps them in step with the main Maps
 * surface (the context listener, the on-show context request, and the initial
 * loads). Holding this here keeps the overlay route a thin shell over the
 * feature module, the same split `mapsController` and `navigationHudController`
 * use.
 *
 * Ordering matters: the context listener and the context request are wired
 * FIRST, using only core event/window APIs that are ready immediately. The
 * facade-backed reads run last and retry, because this window is pre-spawned
 * and mounts during application startup, inside the backend's brief not-ready
 * window: a fallible read placed first would reject and strand the overlay
 * without a listener, so it would never learn the open map.
 */

import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { getMapViews, getPlanetMaps, type MapView, type PlanetMap } from '$lib/api';
import {
	acceptCartographyContextBroadcast,
	CARTOGRAPHY_OVERLAY_CHANGED_EVENT,
	cartographyOverlay,
	loadCartographyConfigs,
	requestCartographyContext,
} from './cartographyOverlay.svelte';

export function createCartographyOverlayController() {
	let planets = $state<PlanetMap[]>([]);
	let views = $state<MapView[]>([]);
	let viewsRefreshEpoch = 0;
	const calibratedPlanets = $derived(planets.filter((planet) => planet.calibration !== null));
	const selectedMapName = $derived(
		cartographyOverlay.context.mapViewId === null
			? 'Default'
			: (views.find((view) => view.id === cartographyOverlay.context.mapViewId)?.name ??
					'Selected map'),
	);

	async function refreshViews(): Promise<void> {
		const planet = cartographyOverlay.context.planet;
		const epoch = ++viewsRefreshEpoch;
		if (!planet) {
			views = [];
			return;
		}
		try {
			const loadedViews = await getMapViews(planet);
			if (epoch === viewsRefreshEpoch && planet === cartographyOverlay.context.planet) {
				views = loadedViews;
			}
		} catch {
			// Preserve the last-good view list; a later event can restore live state.
		}
	}

	// The calibrated-planet list gates pin dropping. Retry through the startup
	// not-ready window so it populates once the backend has composed.
	async function loadCalibratedPlanets(isMounted: () => boolean): Promise<void> {
		for (let attempt = 0; attempt < 40 && isMounted(); attempt++) {
			try {
				const loaded = await getPlanetMaps();
				if (!isMounted()) return;
				planets = loaded;
				return;
			} catch {
				await new Promise((resolve) => setTimeout(resolve, 500));
			}
		}
	}

	/** Wire the context listener + initial loads; returns the teardown. */
	function start(): () => void {
		let mounted = true;
		let unlisten: (() => void) | undefined;
		let unlistenFocus: (() => void) | undefined;
		void (async () => {
			const stopListening = await listen(CARTOGRAPHY_OVERLAY_CHANGED_EVENT, (event) => {
				acceptCartographyContextBroadcast(event.payload);
				void refreshViews();
				void loadCartographyConfigs();
			});
			if (!mounted) return stopListening();
			unlisten = stopListening;
			// Ask the main surface to publish the current context now that the
			// listener is live, and again on every show (focus), so the palette
			// tracks the open map regardless of broadcast timing.
			requestCartographyContext();
			const stopFocus = await getCurrentWindow().onFocusChanged(({ payload: focused }) => {
				if (focused) requestCartographyContext();
			});
			if (!mounted) return stopFocus();
			unlistenFocus = stopFocus;
			// Facade-backed loads last, resilient to the startup not-ready window.
			await loadCalibratedPlanets(() => mounted);
			await refreshViews();
			await loadCartographyConfigs();
		})();
		return () => {
			mounted = false;
			viewsRefreshEpoch++;
			unlisten?.();
			unlistenFocus?.();
		};
	}

	return {
		get planets() {
			return planets;
		},
		get views() {
			return views;
		},
		get calibratedPlanets() {
			return calibratedPlanets;
		},
		get selectedMapName() {
			return selectedMapName;
		},
		start,
	};
}
