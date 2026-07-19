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
	import NavigationSetupModal from '$lib/features/maps/NavigationSetupModal.svelte';
	import RadarCalibrationModal from '$lib/features/maps/RadarCalibrationModal.svelte';
	import PinEditModal from '$lib/features/maps/PinEditModal.svelte';
	import type { PinFormValues } from '$lib/features/maps/PinEditModal.svelte';
	import { createMapsModel } from '$lib/features/maps/mapsModel.svelte';
	import {
		formatWaypoint,
		type WaypointCopyResult,
	} from '$lib/features/maps/waypoint';
	import type { GamePoint } from '$lib/features/maps/coords';
	import type { MapFocusRequest } from '$lib/features/maps/mapTools';
	import type { MapPin, MapView, NavigationRun } from '$lib/api';
	import { describeError } from '$lib/view/errorState';
	import {
		getNearbyMapPin,
		getNavigationSnapshot,
		showNavigationOverlays,
		toggleCartographyOverlay,
	} from '$lib/api';
	import { listen } from '@tauri-apps/api/event';
	import { broadcastCartographyContext } from '$lib/features/maps/cartographyOverlay.svelte';

	const model = createMapsModel();
	let navigation = $state<NavigationRun | null>(null);

	const selectedMapName = $derived(
		model.selectedViewId === null
			? 'Default'
			: model.views.find((view) => view.id === model.selectedViewId)?.name ?? 'Selected map',
	);

	onMount(() => {
		const stopMapsSync = startMapsCartographySync(model);
		let unlisten: (() => void) | undefined;
		void model.loadPlanets();
		void getNavigationSnapshot().then((run) => {
			navigation = run;
			if (run?.status === 'active' || run?.status === 'paused') void showNavigationOverlays();
		}).catch(() => {});
		void listen('navigation:updated', () => {
			void getNavigationSnapshot().then((run) => (navigation = run)).catch(() => {});
			// A recorded visit changes a pin's cooldown, so refresh the pins the
			// hover cards read from.
			void model.refreshPins();
		}).then((stop) => (unlisten = stop));
		return () => {
			stopMapsSync();
			unlisten?.();
		};
	});

	// Publish the active planet/map-view context to the overlay window whenever
	// the selection changes, so its palette tracks the map on screen.
	$effect(() => {
		broadcastCartographyContext({
			planet: model.selected?.name ?? null,
			mapViewId: model.selectedViewId,
		});
	});

	// A configuration change can restyle or remove placed pins; refresh them and
	// re-publish the context so the overlay reloads its palette.
	function onConfigsChanged() {
		void model.refreshPins();
		broadcastCartographyContext({
			planet: model.selected?.name ?? null,
			mapViewId: model.selectedViewId,
		});
	}

	// The pin form: create mode carries the drop point (from a map click
	// or a coordinate scan, which may add an altitude), edit mode the pin
	// being edited (its position is not editable in the form).
	let formOpen = $state(false);
	let dropPoint = $state<GamePoint>({ lon: 0, lat: 0 });
	let dropAltitude = $state<number | null>(null);
	let editingPin = $state<MapPin | null>(null);
	let calibrationOpen = $state(false);
	let overlayConfigOpen = $state(false);
	let navigationSetupOpen = $state(false);
	let radarCalibrationOpen = $state(false);
	let focusRequest = $state<MapFocusRequest | null>(null);
	let focusNonce = 0;

	async function selectPlanet(name: string) {
		focusRequest = null;
		await model.selectPlanet(name);
	}

	async function selectView(id: number | null) {
		await model.selectView(id);
	}

	async function addView(): Promise<MapView | null> {
		try {
			return await model.addView();
		} catch (e) {
			flash(describeError(e, 'The map could not be created'));
			return null;
		}
	}

	async function renameView(id: number, name: string): Promise<boolean> {
		try {
			await model.renameView(id, name);
			// The view id is unchanged, so the selection effect does not fire;
			// re-publish the context so the overlay reloads the renamed view.
			broadcastCartographyContext({
				planet: model.selected?.name ?? null,
				mapViewId: model.selectedViewId,
			});
			return true;
		} catch (e) {
			flash(describeError(e, 'The map could not be renamed'));
			return false;
		}
	}

	async function deleteView(view: MapView): Promise<boolean> {
		try {
			await model.removeView(view.id);
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
				const nearby = await getNearbyMapPin(
					model.selected.name,
					model.selectedViewId,
					dropPoint.lon,
					dropPoint.lat,
				);
				const allowNearby = nearby
					? window.confirm(`"${nearby.pin.name}" is ${nearby.distance.toFixed(2)} units away. Create another pin here anyway?`)
					: false;
				if (nearby && !allowNearby) return false;
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
					pinConfigId: values.configId ?? null,
					allowNearby,
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
				onroute={() => (navigationSetupOpen = true)}
				onradarcalibrate={() => (radarCalibrationOpen = true)}
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
				{navigation}
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
	planet={model.selected?.name ?? null}
	mapViewId={model.selectedViewId}
	mapName={selectedMapName}
	onchanged={onConfigsChanged}
/>

<PinEditModal
	bind:open={formOpen}
	point={dropPoint}
	editing={editingPin}
	planet={model.selected?.name ?? null}
	mapViewId={model.selectedViewId}
	onsubmit={submitPinForm}
/>
<CalibrationModal bind:open={calibrationOpen} />
{#if model.selected}
	<NavigationSetupModal
		bind:open={navigationSetupOpen}
		planet={model.selected.name}
		mapViewId={model.selectedViewId}
		pins={model.pins}
		onstarted={(run) => {
			navigation = run;
			void showNavigationOverlays();
		}}
	/>
{/if}
<RadarCalibrationModal bind:open={radarCalibrationOpen} oncomplete={() => {
	if (navigation) void showNavigationOverlays();
}} />
