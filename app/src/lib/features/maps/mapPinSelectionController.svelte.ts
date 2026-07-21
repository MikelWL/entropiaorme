import type { MapsModel } from './mapsModel.svelte';
import type { ImageRect } from './routeAreaSelection';
import { createRouteAreaSelectionController } from './routeAreaSelectionController.svelte';

export function createMapPinSelectionController(
	model: MapsModel,
	flash: (message: string) => void,
	confirmDelete: (message: string) => boolean = (message) => window.confirm(message),
) {
	let active = $state(false);
	let regions = $state<ImageRect[]>([]);
	let busy = $state(false);

	function begin() {
		regions = [];
		active = true;
	}

	function finish() {
		active = false;
		regions = [];
	}

	async function deleteSelected(pinIds: number[]) {
		const ids = [...new Set(pinIds)].filter((id) => Number.isSafeInteger(id) && id > 0);
		if (busy || ids.length === 0) return;
		const label = ids.length === 1 ? 'this pin' : `these ${ids.length} pins`;
		if (!confirmDelete(`Delete ${label}? This cannot be undone.`)) return;
		busy = true;
		try {
			await model.removePins(ids);
			flash(ids.length === 1 ? '1 pin deleted.' : `${ids.length} pins deleted.`);
			finish();
		} catch {
			flash('The selected pins could not all be deleted.');
		} finally {
			busy = false;
		}
	}

	async function cooldownSelected(pinIds: number[]) {
		const ids = [...new Set(pinIds)].filter((id) => Number.isSafeInteger(id) && id > 0);
		if (busy || ids.length === 0) return;
		busy = true;
		try {
			await model.cooldownPins(ids);
			flash(ids.length === 1 ? '1 tree put on cooldown.' : `${ids.length} trees put on cooldown.`);
			finish();
		} catch {
			flash('The selected trees could not all be put on cooldown.');
		} finally {
			busy = false;
		}
	}

	return {
		get active() {
			return active;
		},
		get regions() {
			return regions;
		},
		get busy() {
			return busy;
		},
		begin,
		cancel: finish,
		setRegions(next: ImageRect[]) {
			regions = next;
		},
		clearRegions() {
			regions = [];
		},
		deleteSelected,
		cooldownSelected,
	};
}

export function createMapAreaSelectionController(
	model: MapsModel,
	flash: (message: string) => void,
) {
	const route = createRouteAreaSelectionController(model);
	const pins = createMapPinSelectionController(model, flash);
	const mode = $derived<'route' | 'pins' | null>(
		route.active ? 'route' : pins.active ? 'pins' : null,
	);

	$effect(() => {
		if (route.active && pins.active) pins.cancel();
	});

	return {
		get active() {
			return mode !== null;
		},
		get mode() {
			return mode;
		},
		get regions() {
			return route.active ? route.regions : pins.regions;
		},
		mount: route.mount,
		beginPinSelection: pins.begin,
		reconcileContext() {
			pins.cancel();
			route.reconcileContext();
		},
		setRegions(next: ImageRect[]) {
			if (route.active) route.setRegions(next);
			else pins.setRegions(next);
		},
		clearRegions() {
			if (route.active) route.clearRegions();
			else pins.clearRegions();
		},
		cancel() {
			if (route.active) void route.cancel();
			else pins.cancel();
		},
		confirmRoute(pinIds: number[]) {
			void route.confirm(pinIds);
		},
		deletePins(pinIds: number[]) {
			void pins.deleteSelected(pinIds);
		},
		cooldownPins(pinIds: number[]) {
			void pins.cooldownSelected(pinIds);
		},
	};
}
