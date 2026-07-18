import { listen } from '@tauri-apps/api/event';
import {
	acceptCartographyOverlayBroadcast,
	CARTOGRAPHY_OVERLAY_CHANGED_EVENT,
	cartographyOverlayConfig,
	MAP_PINS_CHANGED_EVENT,
	setCartographyOverlayConfig,
} from './cartographyOverlay.svelte';
import type { MapsModel } from './mapsModel.svelte';

/**
 * Connects the Maps view model to the cartography preference and overlay
 * events for one mounted route lifetime.
 */
export function startMapsCartographySync(model: MapsModel): () => void {
	let mounted = true;
	let unlistenPins: (() => void) | undefined;
	let unlistenConfig: (() => void) | undefined;
	void (async () => {
		await model.loadPlanets();
		if (!mounted) return;
		const preferredPlanet = cartographyOverlayConfig.current.planet;
		const preferredViewId = cartographyOverlayConfig.current.mapViewId;
		if (preferredPlanet && model.planets.some((planet) => planet.name === preferredPlanet)) {
			await model.selectPlanet(preferredPlanet);
		}
		if (preferredViewId !== null && model.views.some((view) => view.id === preferredViewId)) {
			await model.selectView(preferredViewId);
		}
		if (
			model.selected &&
			(cartographyOverlayConfig.current.planet !== model.selected.name ||
				cartographyOverlayConfig.current.mapViewId !== model.selectedViewId)
		) {
			await setCartographyOverlayConfig({
				...cartographyOverlayConfig.current,
				planet: model.selected.name,
				mapViewId: model.selectedViewId,
			});
		}
		if (!mounted) return;
		const stopPins = await listen<{ planet?: string }>(MAP_PINS_CHANGED_EVENT, (event) => {
			if (event.payload?.planet === model.selected?.name) void model.refreshPins();
		});
		if (!mounted) {
			stopPins();
			return;
		}
		unlistenPins = stopPins;
		const stopConfig = await listen(CARTOGRAPHY_OVERLAY_CHANGED_EVENT, (event) => {
			acceptCartographyOverlayBroadcast(event.payload);
			const planet = cartographyOverlayConfig.current.planet;
			if (planet && planet !== model.selected?.name) void model.selectPlanet(planet);
		});
		if (mounted) unlistenConfig = stopConfig;
		else stopConfig();
	})();
	return () => {
		mounted = false;
		unlistenPins?.();
		unlistenConfig?.();
	};
}
