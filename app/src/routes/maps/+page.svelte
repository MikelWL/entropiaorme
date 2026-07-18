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
	import { formatGamePoint, inBounds, type GamePoint } from '$lib/features/maps/coords';
	import {
		filterMapPins,
		parseGamePointInput,
		type MapFocusRequest,
	} from '$lib/features/maps/mapTools';
	import type { MapPin } from '$lib/api';
	import { scanMapCoordinates } from '$lib/api';
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
	let scanning = $state(false);
	let searchQuery = $state('');
	let coordinateInput = $state('');
	let showGrid = $state(false);
	let focusRequest = $state<MapFocusRequest | null>(null);
	let focusNonce = 0;
	const visiblePins = $derived(filterMapPins(model.pins, searchQuery));

	async function selectPlanet(name: string) {
		searchQuery = '';
		focusRequest = null;
		await model.selectPlanet(name);
		if (model.selected?.calibration) {
			await setCartographyOverlayConfig({ ...cartographyOverlayConfig.current, planet: name });
		}
	}

	function focusMap(point: GamePoint) {
		focusRequest = { point, nonce: ++focusNonce };
	}

	function focusFirstSearchResult() {
		const first = visiblePins[0];
		if (first) focusMap({ lon: first.lon, lat: first.lat });
	}

	function goToCoordinate() {
		const point = parseGamePointInput(coordinateInput);
		if (!point) {
			flash('Enter coordinates as longitude, latitude.');
			return;
		}
		const calibration = model.selected?.calibration;
		if (!calibration || !inBounds(calibration, point)) {
			flash(`That coordinate is outside ${model.selected?.name ?? 'the selected map'}.`);
			return;
		}
		focusMap(point);
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

	// One-click pin drop: scan the calibrated on-screen readout, gate it
	// against the selected planet, and pre-fill the pin form. Every
	// failure leg gets its own actionable message; a wrong read never
	// becomes a pin.
	async function scanMyLocation() {
		if (scanning) return;
		scanning = true;
		try {
			const result = await scanMapCoordinates(model.selected?.name ?? null);
			switch (result.status) {
				case 'read':
					openDropForm(
						{ lon: result.lon ?? 0, lat: result.lat ?? 0 },
						result.altitude ?? null,
					);
					break;
				case 'noRegion':
					flash('The coordinate capture region is not calibrated yet.');
					calibrationOpen = true;
					break;
				case 'captureFailed':
					flash(
						'The screen could not be captured. With several monitors, the screen-share grant covers one of them: the game and its readout must be on the shared monitor.',
					);
					break;
				case 'engineUnavailable':
					flash('The text recogniser is unavailable, so the readout cannot be scanned.');
					break;
				case 'unreadable':
					flash(
						`The capture region did not read as coordinates (saw: "${result.rawText ?? ''}"); recalibrate if the minimap moved.`,
					);
					break;
				case 'implausible':
					flash(
						`Read ${formatGamePoint({ lon: result.lon ?? 0, lat: result.lat ?? 0 })}, which is outside ${model.selected?.name}'s map; is the right planet selected?`,
					);
					break;
			}
		} catch (e) {
			flash(describeError(e, 'The coordinate scan failed'));
		} finally {
			scanning = false;
		}
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
	<header class="flex flex-col gap-1.5">
			<h1 class="text-xl font-semibold text-text tracking-tight">Maps</h1>
			<span class="block h-px w-12 bg-gradient-to-r from-accent/60 to-transparent"></span>
			<p class="text-sm text-text-secondary mt-0.5">
				Explore planet maps, record locations, and copy waypoints back into the game.
			</p>
	</header>

	{#if model.planets.length > 0}
		<MapControls
			planets={model.planets}
			selectedName={model.selected?.name ?? ''}
			{scanning}
			bind:search={searchQuery}
			bind:coordinate={coordinateInput}
			bind:showGrid
			visiblePins={visiblePins.length}
			totalPins={model.pins.length}
			onselectplanet={(name) => void selectPlanet(name)}
			onscan={() => void scanMyLocation()}
			ontoggleoverlay={() => void toggleCartographyOverlay()}
			onconfigure={() => (overlayConfigOpen = true)}
			oncalibrate={() => (calibrationOpen = true)}
			onsearchenter={focusFirstSearchResult}
			ongoto={goToCoordinate}
		/>
	{/if}

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
				imageUrl={model.imageUrl}
				pins={visiblePins}
				{showGrid}
				{focusRequest}
				onmapclick={openDropForm}
				oncopywaypoint={copyWaypoint}
				oneditpin={openEditForm}
				ondeletepin={deletePin}
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
