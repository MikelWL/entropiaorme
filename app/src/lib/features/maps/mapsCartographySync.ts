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
	let configEpoch = 0;

	async function applyConfiguredSelection(epoch: number): Promise<void> {
		const { planet, mapViewId } = cartographyOverlayConfig.current;
		if (!planet || !model.planets.some((candidate) => candidate.name === planet)) return;
		if (planet !== model.selected?.name) await model.selectPlanet(planet);
		if (epoch !== configEpoch || planet !== model.selected?.name) return;
		const viewId =
			mapViewId !== null && model.views.some((view) => view.id === mapViewId) ? mapViewId : null;
		if (viewId !== model.selectedViewId) await model.selectView(viewId);
	}

	void (async () => {
		await model.loadPlanets();
		if (!mounted) return;
		await applyConfiguredSelection(configEpoch);
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
			void applyConfiguredSelection(++configEpoch);
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
