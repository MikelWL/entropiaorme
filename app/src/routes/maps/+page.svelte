<script lang="ts">
	/**
	 * The maps route shell: planet selection over the bundled catalogue,
	 * the pan/zoom viewer, and the pin lifecycle (drop by map click,
	 * edit, delete, copy waypoint). Data and CRUD live in the maps
	 * feature model; geometry lives in the feature's pure modules.
	 */
	import { onMount } from 'svelte';
	import ErrorNotice from '$lib/components/ErrorNotice.svelte';
	import CalibrationModal from '$lib/features/maps/CalibrationModal.svelte';
	import CartographyOverlayModal from '$lib/features/maps/CartographyOverlayModal.svelte';
	import MapControls from '$lib/features/maps/MapControls.svelte';
	import { startMapsCartographySync } from '$lib/features/maps/mapsCartographySync';
	import MapViewer from '$lib/features/maps/MapViewer.svelte';
	import PinEditModal from '$lib/features/maps/PinEditModal.svelte';
	import type { PinFormValues } from '$lib/features/maps/PinEditModal.svelte';
	import { createMapsModel } from '$lib/features/maps/mapsModel.svelte';
	import {
		formatWaypoint,
		type WaypointCopyResult,
	} from '$lib/features/maps/waypoint';
	import type { GamePoint } from '$lib/features/maps/coords';
	import type { MapFocusRequest } from '$lib/features/maps/mapTools';
	import type { MapPin, MapView } from '$lib/api';
	import { describeError } from '$lib/view/errorState';
	import { toggleCartographyOverlay } from '$lib/api';
	import {
		cartographyOverlayConfig,
		setCartographyOverlayConfig,
	} from '$lib/features/maps/cartographyOverlay.svelte';

	const model = createMapsModel();
	onMount(() => startMapsCartographySync(model));

	// The pin form: create mode carries the drop point (from a map click
	// or a coordinate scan, which may add an altitude), edit mode the pin
	// being edited (its position is not editable in the form).
	let formOpen = $state(false);
	let dropPoint = $state<GamePoint>({ lon: 0, lat: 0 });
	let dropAltitude = $state<number | null>(null);
	let editingPin = $state<MapPin | null>(null);
	let calibrationOpen = $state(false);
	let overlayConfigOpen = $state(false);
	let focusRequest = $state<MapFocusRequest | null>(null);
	let focusNonce = 0;

	async function selectPlanet(name: string) {
		focusRequest = null;
		await model.selectPlanet(name);
		if (model.selected?.calibration) {
			await setCartographyOverlayConfig({
				...cartographyOverlayConfig.current,
				planet: name,
				mapViewId: null,
			});
		}
	}

	async function selectView(id: number | null) {
		await model.selectView(id);
		await setCartographyOverlayConfig({
			...cartographyOverlayConfig.current,
			mapViewId: model.selectedViewId,
		});
	}

	async function addView(): Promise<MapView | null> {
		try {
			const created = await model.addView();
			if (created) {
				await setCartographyOverlayConfig({
					...cartographyOverlayConfig.current,
					mapViewId: created.id,
				});
			}
			return created;
		} catch (e) {
			flash(describeError(e, 'The map could not be created'));
			return null;
		}
	}

	async function renameView(id: number, name: string): Promise<boolean> {
		try {
			await model.renameView(id, name);
			// The overlay owns a separate window and reloads its view names
			// when the shared configuration changes.
			await setCartographyOverlayConfig({ ...cartographyOverlayConfig.current });
			return true;
		} catch (e) {
			flash(describeError(e, 'The map could not be renamed'));
			return false;
		}
	}

	async function deleteView(view: MapView): Promise<boolean> {
		try {
			await model.removeView(view.id);
			await setCartographyOverlayConfig({
				...cartographyOverlayConfig.current,
				mapViewId: model.selectedViewId,
			});
			return true;
		} catch (e) {
			flash(describeError(e, 'The map could not be deleted'));
			return false;
		}
	}

	function focusMap(point: GamePoint) {
		focusRequest = { point, nonce: ++focusNonce };
	}

	// Transient action feedback (copy confirmations, CRUD failures).
	let feedback = $state<string | null>(null);
	let feedbackTimer: ReturnType<typeof setTimeout> | null = null;
	function flash(message: string) {
		feedback = message;
		if (feedbackTimer) clearTimeout(feedbackTimer);
		feedbackTimer = setTimeout(() => (feedback = null), 4000);
	}

	function openDropForm(point: GamePoint, altitude: number | null = null) {
		dropPoint = point;
		dropAltitude = altitude;
		editingPin = null;
		formOpen = true;
	}

	function openEditForm(pin: MapPin) {
		dropPoint = { lon: pin.lon, lat: pin.lat };
		editingPin = pin;
		formOpen = true;
	}

	async function submitPinForm(values: PinFormValues): Promise<boolean> {
		try {
			if (editingPin) {
				await model.editPin(editingPin.id, {
					name: values.name,
					icon: values.icon,
					kind: values.kind,
					radiusM: values.radiusM,
					notes: values.notes || null,
				});
				flash(`Pin "${values.name}" updated.`);
			} else if (model.selected) {
				await model.addPin({
					planet: model.selected.name,
					lon: dropPoint.lon,
					lat: dropPoint.lat,
					altitude: dropAltitude,
					name: values.name,
					icon: values.icon,
					kind: values.kind,
					radiusM: values.radiusM,
					notes: values.notes || null,
					sessionId: null,
					mapViewId: model.selectedViewId,
				});
				flash(`Pin "${values.name}" dropped.`);
			}
			return true;
		} catch (e) {
			flash(describeError(e, 'The pin could not be saved'));
			return false;
		}
	}

	async function deletePin(pin: MapPin) {
		try {
			await model.removePin(pin.id);
			flash(`Pin "${pin.name}" deleted.`);
		} catch (e) {
			flash(describeError(e, 'The pin could not be deleted'));
		}
	}

	async function copyWaypoint(pin: MapPin): Promise<WaypointCopyResult> {
		const waypoint = formatWaypoint({
			technicalName: model.selected?.technicalName ?? null,
			lon: pin.lon,
			lat: pin.lat,
			altitude: pin.altitude,
			label: pin.name,
		});
		if (!waypoint) {
			return { message: 'Waypoint unavailable', copied: false };
		}
		try {
			await navigator.clipboard.writeText(waypoint);
			return { message: 'Waypoint copied.', copied: true };
		} catch {
			return { message: 'Copy failed', copied: false };
		}
	}
</script>

<div class="flex h-full flex-col gap-3 px-4 pb-4 sm:px-6 sm:pb-6">
	<header class="flex flex-col items-start justify-between gap-3 sm:flex-row sm:items-end">
		<div class="flex min-w-0 flex-col gap-1.5">
			<h1 class="text-xl font-semibold text-text tracking-tight">Maps</h1>
			<span class="block h-px w-12 bg-gradient-to-r from-accent/60 to-transparent"></span>
			<p class="mt-0.5 text-sm text-text-secondary">
				Explore planet maps, record locations, and copy waypoints back into the game.
			</p>
		</div>
		{#if model.planets.length > 0}
			<MapControls
				pins={model.pins}
				ontoggleoverlay={() => void toggleCartographyOverlay()}
				onconfigure={() => (overlayConfigOpen = true)}
				oncalibrate={() => (calibrationOpen = true)}
				onselectpin={(pin) => focusMap({ lon: pin.lon, lat: pin.lat })}
			/>
		{/if}
	</header>

	<div class="h-4 shrink-0">
		{#if feedback}<p class="truncate text-xs text-text-secondary" role="status">{feedback}</p>{/if}
	</div>

	{#if model.error}
		<ErrorNotice message={model.error} />
	{:else if !model.loading && model.planets.length === 0}
		<p class="text-sm text-text-secondary">
			No planet maps are bundled with this installation, so the maps surface is unavailable.
		</p>
	{/if}

	<div class="min-h-0 flex-1">
		{#if model.selected && model.imageUrl}
			<MapViewer
				planet={model.selected}
				planets={model.planets}
				imageUrl={model.imageUrl}
				pins={model.pins}
				views={model.views}
				selectedViewId={model.selectedViewId}
				{focusRequest}
				onmapclick={openDropForm}
				oncopywaypoint={copyWaypoint}
				oneditpin={openEditForm}
				ondeletepin={deletePin}
				onselectplanet={(name) => void selectPlanet(name)}
				onselectview={(id) => void selectView(id)}
				onaddview={addView}
				onrenameview={renameView}
				ondeleteview={deleteView}
			/>
		{:else if model.loading}
			<div
				class="flex h-full items-center justify-center rounded-lg border border-border bg-base text-sm text-text-secondary"
			>
				Loading map…
			</div>
		{/if}
	</div>
</div>

<CartographyOverlayModal
	bind:open={overlayConfigOpen}
	config={cartographyOverlayConfig.current}
/>

<PinEditModal bind:open={formOpen} point={dropPoint} editing={editingPin} onsubmit={submitPinForm} />
<CalibrationModal bind:open={calibrationOpen} />
