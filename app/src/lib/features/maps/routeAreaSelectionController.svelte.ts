import { listen } from '@tauri-apps/api/event';
import { hideNavigationOverlays, showNavigationOverlays } from '$lib/api';
import type { MapsModel } from './mapsModel.svelte';
import {
	acceptRouteAreaSelectionRequest,
	emitRouteAreaSelectionCancelled,
	emitRouteAreaSelectionResult,
	type ImageRect,
	ROUTE_AREA_SELECTION_REQUEST_EVENT,
	ROUTE_AREA_SELECTION_RESET_EVENT,
} from './routeAreaSelection';

type SelectionContext = { planet: string; mapViewId: number | null };

export function createRouteAreaSelectionController(model: MapsModel) {
	let requestId = $state<number | null>(null);
	let requestContext = $state<SelectionContext | null>(null);
	let regions = $state<ImageRect[]>([]);
	let confirmedRegions = $state<ImageRect[]>([]);
	let confirmedContext = $state<SelectionContext | null>(null);

	const active = $derived(requestId != null);

	function currentContext(): SelectionContext | null {
		const planet = model.selected?.name;
		return planet ? { planet, mapViewId: model.selectedViewId } : null;
	}

	function sameContext(left: SelectionContext | null, right: SelectionContext): boolean {
		return left?.planet === right.planet && left.mapViewId === right.mapViewId;
	}

	function clear() {
		requestId = null;
		requestContext = null;
		regions = [];
		confirmedRegions = [];
		confirmedContext = null;
	}

	function reconcileContext() {
		const context = currentContext();
		if (!context || (requestContext && !sameContext(requestContext, context))) {
			const cancelledRequest = requestId;
			clear();
			if (cancelledRequest != null) emitRouteAreaSelectionCancelled(cancelledRequest);
			void showNavigationOverlays();
			return;
		}
		if (confirmedContext && !sameContext(confirmedContext, context)) clear();
	}

	async function acceptRequest(payload: unknown) {
		const request = acceptRouteAreaSelectionRequest(payload);
		if (!request) return;
		const context = currentContext();
		if (!context || !sameContext(context, request)) {
			emitRouteAreaSelectionCancelled(request.requestId);
			await showNavigationOverlays();
			return;
		}
		requestId = request.requestId;
		requestContext = context;
		regions = sameContext(confirmedContext, request) ? [...confirmedRegions] : [];
		await hideNavigationOverlays();
	}

	function mount(): () => void {
		let mounted = true;
		let unlistenRequest: (() => void) | undefined;
		let unlistenReset: (() => void) | undefined;
		void listen(
			ROUTE_AREA_SELECTION_REQUEST_EVENT,
			(event) => void acceptRequest(event.payload),
		).then((stop) => {
			if (mounted) unlistenRequest = stop;
			else stop();
		});
		void listen(ROUTE_AREA_SELECTION_RESET_EVENT, clear).then((stop) => {
			if (mounted) unlistenReset = stop;
			else stop();
		});
		return () => {
			mounted = false;
			unlistenRequest?.();
			unlistenReset?.();
		};
	}

	async function confirm(pinIds: number[]) {
		const context = currentContext();
		const selected = [...new Set(pinIds)]
			.filter((id) => Number.isSafeInteger(id) && id > 0)
			.sort((left, right) => left - right);
		if (requestId == null || !context || selected.length === 0) return;
		confirmedRegions = [...regions];
		confirmedContext = context;
		emitRouteAreaSelectionResult({ requestId, ...context, pinIds: selected });
		requestId = null;
		requestContext = null;
		await showNavigationOverlays();
	}

	async function cancel() {
		if (requestId == null) return;
		emitRouteAreaSelectionCancelled(requestId);
		requestId = null;
		requestContext = null;
		regions = [...confirmedRegions];
		await showNavigationOverlays();
	}

	return {
		get active() {
			return active;
		},
		get regions() {
			return regions;
		},
		setRegions(next: ImageRect[]) {
			regions = next;
		},
		clearRegions() {
			regions = [];
		},
		mount,
		reconcileContext,
		confirm,
		cancel,
	};
}
