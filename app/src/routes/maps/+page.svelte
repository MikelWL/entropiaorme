<script lang="ts">
	/**
	 * The maps route shell: planet selection over the bundled catalogue,
	 * the pan/zoom viewer, and the pin lifecycle (drop by map click,
	 * edit, delete, copy waypoint). Data and CRUD live in the maps
	 * feature model; geometry lives in the feature's pure modules.
	 */
	import { onMount } from 'svelte';
	import { listen } from '@tauri-apps/api/event';
	import Button from '$lib/components/Button.svelte';
	import ErrorNotice from '$lib/components/ErrorNotice.svelte';
	import Select from '$lib/components/Select.svelte';
	import CalibrationModal from '$lib/features/maps/CalibrationModal.svelte';
	import CartographyOverlayModal from '$lib/features/maps/CartographyOverlayModal.svelte';
	import MapViewer from '$lib/features/maps/MapViewer.svelte';
	import PinEditModal from '$lib/features/maps/PinEditModal.svelte';
	import type { PinFormValues } from '$lib/features/maps/PinEditModal.svelte';
	import { createMapsModel } from '$lib/features/maps/mapsModel.svelte';
	import {
		formatWaypoint,
		type WaypointCopyResult,
	} from '$lib/features/maps/waypoint';
	import { formatGamePoint, type GamePoint } from '$lib/features/maps/coords';
	import type { MapPin } from '$lib/api';
	import { scanMapCoordinates } from '$lib/api';
	import { describeError } from '$lib/view/errorState';
	import { toggleCartographyOverlay } from '$lib/api';
	import {
		acceptCartographyOverlayBroadcast,
		cartographyOverlayConfig,
		CARTOGRAPHY_OVERLAY_CHANGED_EVENT,
		MAP_PINS_CHANGED_EVENT,
		setCartographyOverlayConfig,
	} from '$lib/features/maps/cartographyOverlay.svelte';

	const model = createMapsModel();
	onMount(() => {
		let mounted = true;
		let unlistenPins: (() => void) | undefined;
		let unlistenConfig: (() => void) | undefined;
		void (async () => {
			await model.loadPlanets();
			if (!mounted) return;
			const preferred = cartographyOverlayConfig.current.planet;
			if (preferred && model.planets.some((planet) => planet.name === preferred)) {
				await model.selectPlanet(preferred);
			} else if (model.selected) {
				await setCartographyOverlayConfig({
					...cartographyOverlayConfig.current,
					planet: model.selected.name,
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
	});

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

	async function selectPlanet(name: string) {
		await model.selectPlanet(name);
		if (model.selected?.calibration) {
			await setCartographyOverlayConfig({ ...cartographyOverlayConfig.current, planet: name });
		}
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

<div class="flex h-full flex-col px-6 pb-6 gap-4">
	<div class="flex items-end justify-between gap-4">
		<header class="flex flex-col gap-1.5">
			<h1 class="text-xl font-semibold text-text tracking-tight">Maps</h1>
			<span class="block h-px w-12 bg-gradient-to-r from-accent/60 to-transparent"></span>
			<p class="text-sm text-text-secondary mt-0.5">
				Planet maps with your own pins. Click the map to drop a pin; hover a pin for details,
				or click it to copy its waypoint.
			</p>
		</header>
		{#if model.planets.length > 0}
			<div class="flex items-center gap-2 shrink-0">
				<Button size="sm" loading={scanning} onclick={scanMyLocation}>Pin my location</Button>
				<Button size="sm" variant="secondary" onclick={toggleCartographyOverlay}>
					Pin overlay
				</Button>
				<Button size="sm" variant="ghost" onclick={() => (overlayConfigOpen = true)}>
					Configure
				</Button>
				<Button size="sm" variant="secondary" onclick={() => (calibrationOpen = true)}>
					Calibrate capture
				</Button>
				<label class="flex items-center gap-2">
					<span class="text-xs text-text-secondary">Planet</span>
					<Select
						class="w-52"
						value={model.selected?.name ?? ''}
						onchange={(event) =>
							selectPlanet((event.currentTarget as HTMLSelectElement).value)}
					>
						{#each model.planets as planet (planet.name)}
							<option value={planet.name}>
								{planet.name}{planet.calibration ? '' : ' (view-only)'}
							</option>
						{/each}
					</Select>
				</label>
			</div>
		{/if}
	</div>

	{#if feedback}
		<p class="text-xs text-text-secondary" role="status">{feedback}</p>
	{/if}

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
				pins={model.pins}
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
