<script lang="ts">
	/**
	 * The pan/zoom map view: raster, route and high-volume pins share one
	 * canvas and one viewport transform (`./viewport`). Wheel zoom anchors
	 * under the cursor; drag pans; a still click reports the game coordinate
	 * for a pin drop; canvas hit testing raises the detail card.
	 * Arrow keys pan, +/- zoom about the centre, 0 re-fits.
	 */
	import { untrack } from 'svelte';
	import type { MapPin, MapView, NavigationRun, PlanetMap } from '$lib/api';
	import Button from '$lib/components/Button.svelte';
	import Select from '$lib/components/Select.svelte';
	import { markerRgba, pinRenderColour } from './markerColour';
	import { formatGamePoint, gameToImage, imageToGame, type GamePoint } from './coords';
	import type { MapFocusRequest } from './mapTools';
	import MapViewSelector from './MapViewSelector.svelte';
	import { pinGlyph } from './pinIcons';
	import PinCard from './PinCard.svelte';
	import type { WaypointCopyResult } from './waypoint';
	import { externalLinks } from '$lib/utils/openExternal';
	import {
		ZOOM_STEP,
		centreOnImage,
		fitZoom,
		fitViewport,
		imageToView,
		panBy,
		viewToImage,
		zoomAt,
		type Viewport,
	} from './viewport';

	let {
		planet,
		planets,
		imageUrl,
		pins,
		views,
		selectedViewId,
		onmapclick,
		oncopywaypoint,
		oneditpin,
		ondeletepin,
		oncooldownpin,
		onselectplanet,
		onselectview,
		onaddview,
		onrenameview,
		ondeleteview,
		focusRequest = null,
		navigation = null,
	}: {
		planet: PlanetMap;
		planets: PlanetMap[];
		imageUrl: string;
		pins: MapPin[];
		views: MapView[];
		selectedViewId: number | null;
		/** A still click on a calibrated map, in game units. */
		onmapclick: (point: GamePoint) => void;
		oncopywaypoint: (pin: MapPin) => Promise<WaypointCopyResult>;
		oneditpin: (pin: MapPin) => void;
		ondeletepin: (pin: MapPin) => void;
		oncooldownpin: (pin: MapPin) => void;
		onselectplanet: (name: string) => void;
		onselectview: (id: number | null) => void;
		onaddview: () => Promise<MapView | null>;
		onrenameview: (id: number, name: string) => Promise<boolean>;
		ondeleteview: (view: MapView) => Promise<boolean>;
		focusRequest?: MapFocusRequest | null;
		navigation?: NavigationRun | null;
	} = $props();

	const cal = $derived(planet.calibration);

	let viewW = $state(0);
	let viewH = $state(0);
	let canvas = $state<HTMLCanvasElement | null>(null);
	let image = $state<HTMLImageElement | null>(null);
	let vp = $state<Viewport>({ zoom: 1, panX: 0, panY: 0 });
	let cursorPoint = $state<GamePoint | null>(null);
	let handledFocusNonce: number | null = null;
	let markerMode = $state<'auto' | 'icons' | 'precision'>('auto');
	// The map-asset attribution popover: a small note clarifying this surface is
	// for the user's own pins, not a wiki, pointing wiki-style map needs at the
	// asset source. Toggled by its badge, dismissed on any map interaction.
	let attributionOpen = $state(false);

	// The active pin card: raised by marker hover or focus, kept open
	// while the pointer is over the card itself, closed on a short delay
	// so travelling marker -> card does not dismiss it. Clicking a marker
	// *locks* the card: it then stays put regardless of hover, so the pointer
	// can travel off the marker to reach card actions (Put on cooldown, etc.)
	// without the card dismissing. A locked card is released by clicking empty
	// map, clicking another marker (which re-locks to it), or an action that
	// closes the card.
	let activePin = $state<MapPin | null>(null);
	let lockedPinId = $state<number | null>(null);
	let copyFeedback = $state<WaypointCopyResult | null>(null);
	let copyRequest = 0;
	let closeTimer: ReturnType<typeof setTimeout> | null = null;

	function raiseCard(pin: MapPin) {
		if (closeTimer) clearTimeout(closeTimer);
		if (activePin?.id !== pin.id) {
			copyFeedback = null;
			copyRequest += 1;
		}
		activePin = pin;
	}

	function dismissCard() {
		if (closeTimer) clearTimeout(closeTimer);
		lockedPinId = null;
		activePin = null;
		copyFeedback = null;
		copyRequest += 1;
	}

	async function copyPinWaypoint(pin: MapPin) {
		raiseCard(pin);
		const request = ++copyRequest;
		const feedback = await oncopywaypoint(pin);
		if (request === copyRequest && activePin?.id === pin.id) copyFeedback = feedback;
	}

	function scheduleCardClose() {
		// A locked card ignores hover-out: it persists until explicitly dismissed.
		if (lockedPinId != null) return;
		if (closeTimer) clearTimeout(closeTimer);
		closeTimer = setTimeout(() => {
			activePin = null;
			copyFeedback = null;
			copyRequest += 1;
		}, 250);
	}

	// Load the raster whenever the planet's data URL changes, and re-fit
	// the viewport to the new map.
	$effect(() => {
		const url = imageUrl;
		const img = new Image();
		// Drop the previous raster immediately: until the new one loads,
		// nothing may render or hit-test against mismatched map data.
		image = null;
		img.onload = () => {
			image = img;
			vp = fitViewport(img.naturalWidth, img.naturalHeight, viewW, viewH);
		};
		img.onerror = () => {
			image = null;
		};
		img.src = url;
		activePin = null;
		lockedPinId = null;
		return () => {
			img.onload = null;
			img.onerror = null;
		};
	});

	// Search and coordinate tools request focus declaratively. The nonce
	// lets the same point be requested repeatedly after the user pans away.
	$effect(() => {
		const request = focusRequest;
		const img = image;
		if (
			!request ||
			request.nonce === handledFocusNonce ||
			!img ||
			!cal ||
			viewW === 0 ||
			viewH === 0
		)
			return;
		const point = gameToImage(cal, request.point);
		untrack(() => {
			vp = centreOnImage(vp, point, img.naturalWidth, img.naturalHeight, viewW, viewH);
			handledFocusNonce = request.nonce;
		});
	});

	// Re-clamp on container resize. The current viewport is read
	// untracked and reassigned only when the clamp actually moves it:
	// tracking `vp` here would make the effect its own trigger (each
	// reassignment is a fresh object), which pegs the flush loop and
	// wedges the whole page.
	$effect(() => {
		const w = viewW;
		const h = viewH;
		const img = image;
		if (!img || w === 0 || h === 0) return;
		untrack(() => {
			const next = zoomAt(vp, 1, w / 2, h / 2, img.naturalWidth, img.naturalHeight, w, h);
			if (next.zoom !== vp.zoom || next.panX !== vp.panX || next.panY !== vp.panY) {
				vp = next;
			}
		});
	});

	function applyZoom(factor: number, anchorX: number, anchorY: number) {
		if (!image) return;
		vp = zoomAt(
			vp,
			factor,
			anchorX,
			anchorY,
			image.naturalWidth,
			image.naturalHeight,
			viewW,
			viewH,
		);
	}

	function handleWheel(event: WheelEvent) {
		event.preventDefault();
		const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
		const factor = event.deltaY < 0 ? ZOOM_STEP : 1 / ZOOM_STEP;
		applyZoom(factor, event.clientX - rect.left, event.clientY - rect.top);
	}

	// Drag vs click: a press starts a candidate drag; passing the slop
	// threshold makes it a pan, otherwise release is a map click.
	const DRAG_SLOP_PX = 4;
	let pressed = $state(false);
	let dragging = $state(false);
	let lastPointer = { x: 0, y: 0 };
	let pressPointer = { x: 0, y: 0 };

	function handlePointerDown(event: PointerEvent) {
		if (event.button !== 0) return;
		attributionOpen = false;
		(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
		pressed = true;
		dragging = false;
		lastPointer = { x: event.clientX, y: event.clientY };
		pressPointer = lastPointer;
	}

	function handlePointerMove(event: PointerEvent) {
		const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
		if (image && cal) {
			const imagePoint = viewToImage(vp, event.clientX - rect.left, event.clientY - rect.top);
			cursorPoint =
				imagePoint.x >= 0 &&
				imagePoint.y >= 0 &&
				imagePoint.x <= image.naturalWidth &&
				imagePoint.y <= image.naturalHeight
					? imageToGame(cal, imagePoint)
					: null;
		}
		// While a card is locked open (marker was clicked), hover neither switches
		// nor closes it; the locked card owns the surface until it is dismissed.
		if (!pressed && lockedPinId == null) {
			const hit = nearestPlacedPin(event.clientX - rect.left, event.clientY - rect.top, 10);
			if (hit) raiseCard(hit.pin);
			else if (activePin) scheduleCardClose();
		}
		if (!pressed || !image) return;
		const dx = event.clientX - lastPointer.x;
		const dy = event.clientY - lastPointer.y;
		if (
			!dragging &&
			Math.hypot(event.clientX - pressPointer.x, event.clientY - pressPointer.y) < DRAG_SLOP_PX
		) {
			return;
		}
		dragging = true;
		lastPointer = { x: event.clientX, y: event.clientY };
		vp = panBy(vp, dx, dy, image.naturalWidth, image.naturalHeight, viewW, viewH);
	}

	function handlePointerUp(event: PointerEvent) {
		if (!pressed) return;
		pressed = false;
		if (dragging || !cal || !image) {
			dragging = false;
			return;
		}
		const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
		const imagePoint = viewToImage(vp, event.clientX - rect.left, event.clientY - rect.top);
		if (
			imagePoint.x < 0 ||
			imagePoint.y < 0 ||
			imagePoint.x > image.naturalWidth ||
			imagePoint.y > image.naturalHeight
		) {
			return;
		}
		const hit = nearestPlacedPin(event.clientX - rect.left, event.clientY - rect.top, 11);
		if (hit) {
			// Clicking a marker locks its card open so the pointer can travel to
			// the card's actions without the hover-close dismissing it.
			raiseCard(hit.pin);
			lockedPinId = hit.pin.id;
			return;
		}
		if (lockedPinId != null) {
			// A locked card is open: an empty-map click releases it rather than
			// dropping a new pin. A second click then places as usual.
			dismissCard();
			return;
		}
		onmapclick(imageToGame(cal, imagePoint));
	}

	const PAN_STEP_PX = 60;
	function handleKeydown(event: KeyboardEvent) {
		if (event.key === 'Escape' && attributionOpen) {
			attributionOpen = false;
			event.preventDefault();
			return;
		}
		if (!image) return;
		const pan = (dx: number, dy: number) => {
			vp = panBy(vp, dx, dy, image!.naturalWidth, image!.naturalHeight, viewW, viewH);
		};
		switch (event.key) {
			case 'ArrowLeft':
				pan(PAN_STEP_PX, 0);
				break;
			case 'ArrowRight':
				pan(-PAN_STEP_PX, 0);
				break;
			case 'ArrowUp':
				pan(0, PAN_STEP_PX);
				break;
			case 'ArrowDown':
				pan(0, -PAN_STEP_PX);
				break;
			case '+':
			case '=':
				applyZoom(ZOOM_STEP, viewW / 2, viewH / 2);
				break;
			case '-':
			case '_':
				applyZoom(1 / ZOOM_STEP, viewW / 2, viewH / 2);
				break;
			case '0':
				vp = fitViewport(image.naturalWidth, image.naturalHeight, viewW, viewH);
				break;
			default:
				return;
		}
		event.preventDefault();
	}

	interface PlacedPin {
		pin: MapPin;
		x: number;
		y: number;
		radiusRx: number | null;
		radiusRy: number | null;
	}

	// Every pin projected through calibration + viewport. A radius pin's
	// disc is an ellipse in view space on anisotropic maps (the per-axis
	// scales differ), which is the honest rendering, not a bug.
	const placedPins = $derived.by<PlacedPin[]>(() => {
		if (!cal) return [];
		return pins.map((pin) => {
			const img = gameToImage(cal, { lon: pin.lon, lat: pin.lat });
			const view = imageToView(vp, img.x, img.y);
			return {
				pin,
				x: view.x,
				y: view.y,
				radiusRx: pin.radiusM == null ? null : (pin.radiusM / cal.unitsPerPixelX) * vp.zoom,
				radiusRy: pin.radiusM == null ? null : (pin.radiusM / cal.unitsPerPixelY) * vp.zoom,
			};
		}).filter((placed) => placed.x >= -32 && placed.y >= -32 && placed.x <= viewW + 32 && placed.y <= viewH + 32);
	});

	function nearestPlacedPin(x: number, y: number, radius: number): PlacedPin | null {
		let nearest: PlacedPin | null = null;
		let nearestDistance = radius;
		for (const placed of placedPins) {
			const candidate = Math.hypot(placed.x - x, placed.y - y);
			if (candidate <= nearestDistance) {
				nearest = placed;
				nearestDistance = candidate;
			}
		}
		return nearest;
	}

	const activePlaced = $derived(
		activePin ? placedPins.find((placed) => placed.pin.id === activePin?.id) ?? null : null,
	);

	// Raster, route, areas, and high-volume markers share one draw-on-dirty
	// canvas. Marker rendering changes semantically with zoom instead of
	// creating one DOM node per tree.
	$effect(() => {
		const ctx = canvas?.getContext('2d');
		const img = image;
		const visiblePins = placedPins;
		const mode = markerMode;
		const now = Date.now() / 1000;
		const run = navigation;
		if (!ctx || !canvas || !img || viewW === 0 || viewH === 0) return;
		const dpr = window.devicePixelRatio || 1;
		canvas.width = Math.round(viewW * dpr);
		canvas.height = Math.round(viewH * dpr);
		ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
		ctx.clearRect(0, 0, viewW, viewH);
		ctx.imageSmoothingEnabled = true;
		ctx.imageSmoothingQuality = 'high';
		ctx.drawImage(
			img,
			-vp.panX * vp.zoom,
			-vp.panY * vp.zoom,
			img.naturalWidth * vp.zoom,
			img.naturalHeight * vp.zoom,
		);

		for (const placed of visiblePins) {
			if (placed.radiusRx == null || placed.radiusRy == null) continue;
			ctx.beginPath();
			ctx.ellipse(placed.x, placed.y, placed.radiusRx, placed.radiusRy, 0, 0, Math.PI * 2);
			ctx.fillStyle = 'rgba(56, 189, 248, 0.1)';
			ctx.strokeStyle = 'rgba(56, 189, 248, 0.55)';
			ctx.lineWidth = 1.5;
			ctx.fill();
			ctx.stroke();
		}

		if (cal && run?.planet === planet.name && run.mapViewId === selectedViewId) {
			const routePoints = run.stops
				.filter((stop) => stop.status === 'active' || stop.status === 'pending')
				.map((stop) => {
					const point = gameToImage(cal, { lon: stop.lon, lat: stop.lat });
					return { stop, point: imageToView(vp, point.x, point.y) };
				});
			if (routePoints.length > 0) {
				const currentImage = gameToImage(cal, { lon: run.currentLon, lat: run.currentLat });
				const current = imageToView(vp, currentImage.x, currentImage.y);
				ctx.beginPath();
				ctx.moveTo(current.x, current.y);
				for (const item of routePoints) ctx.lineTo(item.point.x, item.point.y);
				ctx.strokeStyle = 'rgba(125, 211, 252, 0.9)';
				ctx.lineWidth = 2;
				ctx.setLineDash([7, 5]);
				ctx.stroke();
				ctx.setLineDash([]);
				ctx.beginPath();
				ctx.arc(current.x, current.y, 5, 0, Math.PI * 2);
				ctx.fillStyle = 'rgba(52, 211, 153, 0.95)';
				ctx.fill();
				ctx.font = '600 9px sans-serif';
				ctx.textAlign = 'center';
				ctx.textBaseline = 'middle';
				for (const item of routePoints) {
					ctx.beginPath();
					ctx.arc(item.point.x, item.point.y, item.stop.status === 'active' ? 8 : 6, 0, Math.PI * 2);
					ctx.fillStyle = item.stop.status === 'active' ? 'rgba(250, 204, 21, 0.95)' : 'rgba(14, 116, 144, 0.9)';
					ctx.fill();
					ctx.fillStyle = 'rgba(255,255,255,0.95)';
					ctx.fillText(String(item.stop.ordinal + 1), item.point.x, item.point.y + 0.5);
				}
			}
		}

		const relativeZoom = vp.zoom / fitZoom(img.naturalWidth, img.naturalHeight, viewW, viewH);
		const effective = mode === 'auto' ? (relativeZoom < 2.5 ? 'density' : 'precision') : mode;
		const onCooldown = (pin: MapPin) =>
			pin.specialKind === 'tree' && pin.cooldownUntil != null && pin.cooldownUntil > now;
		if (effective === 'icons') {
			// Emoji markers; a tree on cooldown is dimmed.
			ctx.font = '18px sans-serif';
			ctx.textAlign = 'center';
			ctx.textBaseline = 'bottom';
			for (const placed of visiblePins) {
				ctx.globalAlpha = onCooldown(placed.pin) ? 0.4 : 1;
				ctx.fillText(pinGlyph(placed.pin.icon), placed.x, placed.y);
			}
			ctx.globalAlpha = 1;
		} else {
			// Fine-grained points in each pin's configured colour (its cooldown
			// colour while a tree is on cooldown). Additive compositing keeps a
			// dense field of overlapping points legible as a heatmap at low zoom.
			const dense = effective === 'density';
			const radius = dense ? 2 : relativeZoom >= 12 ? 2.25 : 2.75;
			const alpha = dense ? 0.5 : relativeZoom >= 12 ? 0.92 : 0.5;
			ctx.globalCompositeOperation = 'lighter';
			for (const placed of visiblePins) {
				const colour = pinRenderColour(
					placed.pin.colour,
					placed.pin.cooldownColour,
					placed.pin.specialKind,
					placed.pin.cooldownUntil,
					now,
				);
				ctx.beginPath();
				ctx.arc(placed.x, placed.y, radius, 0, Math.PI * 2);
				ctx.fillStyle = markerRgba(colour, alpha);
				ctx.fill();
			}
			ctx.globalCompositeOperation = 'source-over';
		}

		if (activePlaced) {
			ctx.beginPath();
			ctx.arc(activePlaced.x, activePlaced.y, 8, 0, Math.PI * 2);
			ctx.strokeStyle = 'rgba(253, 224, 71, 0.95)';
			ctx.lineWidth = 2;
			ctx.stroke();
		}
	});

</script>

<!-- The viewer is a keyboard-operable application surface (arrows pan,
     +/- zoom, 0 fits); role="application" is the honest role, which the
     static a11y rules do not recognise as interactive. -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div
	class="relative h-full w-full overflow-hidden rounded-lg border border-border bg-base select-none"
	bind:clientWidth={viewW}
	bind:clientHeight={viewH}
	role="application"
	aria-label="{planet.name} map. Arrow keys pan, plus and minus zoom, 0 fits the view."
	tabindex="0"
	onwheel={handleWheel}
	onpointerdown={handlePointerDown}
	onpointermove={handlePointerMove}
	onpointerup={handlePointerUp}
	onpointercancel={() => {
		pressed = false;
		dragging = false;
	}}
	onpointerleave={() => (cursorPoint = null)}
	onkeydown={handleKeydown}
>
	<canvas class="absolute inset-0" bind:this={canvas} aria-hidden="true"></canvas>

	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div
		class="absolute left-2 top-2 z-10 w-48 space-y-1 rounded-md border border-border bg-base/85 p-1 shadow-sm backdrop-blur"
		onpointerdown={(event) => event.stopPropagation()}
		onkeydown={(event) => event.stopPropagation()}
	>
		<Select
			value={planet.name}
			aria-label="Planet map"
			onchange={(event) => onselectplanet((event.currentTarget as HTMLSelectElement).value)}
		>
			{#each planets as option (option.name)}
				<option value={option.name}>{option.name}{option.calibration ? '' : ' (view-only)'}</option>
			{/each}
		</Select>
		<MapViewSelector
			{views}
			selectedId={selectedViewId}
			onselect={onselectview}
			onadd={onaddview}
			onrename={onrenameview}
			ondelete={ondeleteview}
		/>
	</div>

	<div
		class="absolute right-2 top-2 z-10 flex items-center gap-1 rounded-md border border-border bg-base/85 p-1 shadow-sm backdrop-blur"
		role="group"
		aria-label="Map zoom"
		onpointerdown={(event) => event.stopPropagation()}
	>
		<Button size="sm" variant="ghost" aria-label="Zoom in" onclick={() => applyZoom(ZOOM_STEP, viewW / 2, viewH / 2)}>+</Button>
		<Button size="sm" variant="ghost" aria-label="Zoom out" onclick={() => applyZoom(1 / ZOOM_STEP, viewW / 2, viewH / 2)}>−</Button>
		<Button size="sm" variant="ghost" aria-label="Fit map to view" onclick={() => {
			if (image) vp = fitViewport(image.naturalWidth, image.naturalHeight, viewW, viewH);
		}}>Fit</Button>
		<Select value={markerMode} aria-label="Pin display" onchange={(event) => markerMode = (event.currentTarget as HTMLSelectElement).value as typeof markerMode}>
			<option value="auto">Auto</option>
			<option value="icons">Icons</option>
			<option value="precision">Dots</option>
		</Select>
	</div>

	{#if activePin && activePlaced}
			<PinCard
				pin={activePin}
				x={activePlaced.x}
				y={activePlaced.y}
				{viewW}
				{viewH}
				technicalName={planet.technicalName}
				{copyFeedback}
				onpointerenter={() => raiseCard(activePin!)}
				onpointerleave={scheduleCardClose}
				onfocusin={() => raiseCard(activePin!)}
				onfocusout={scheduleCardClose}
				oncopy={() => void copyPinWaypoint(activePin!)}
				onedit={() => oneditpin(activePin!)}
				ondelete={() => {
					const pin = activePin!;
					dismissCard();
					ondeletepin(pin);
				}}
				oncooldown={() => oncooldownpin(activePin!)}
			/>
	{/if}

	{#if !cal}
		<p
			class="absolute bottom-3 left-1/2 -translate-x-1/2 rounded-md bg-surface/90 px-3 py-1.5 text-xs text-text-secondary"
		>
			This map has no coordinate calibration yet: it is view-only, so pins cannot be placed.
		</p>
	{/if}

	<!-- Bottom-right cluster: the live coordinate readout joins the zoom
	     multiplier, with the map-asset attribution badge beneath them. -->
	<div class="absolute bottom-2 right-2 z-10 flex flex-col items-end gap-1">
		<div class="flex items-center gap-1">
			{#if cursorPoint}
				<output class="pointer-events-none rounded-md border border-border bg-base/85 px-2 py-1 text-xs tabular-nums text-text shadow-sm backdrop-blur" aria-live="off">
					{formatGamePoint(cursorPoint)}
				</output>
			{/if}
			<output class="pointer-events-none rounded-md border border-border bg-base/85 px-2 py-1 text-[10px] tabular-nums text-text-secondary shadow-sm backdrop-blur" aria-live="off">
				{vp.zoom.toFixed(vp.zoom < 10 ? 2 : 0)}×
			</output>
		</div>
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div class="relative" onpointerdown={(event) => event.stopPropagation()}>
			{#if attributionOpen}
				<div
					class="absolute bottom-full right-0 mb-1.5 w-64 rounded-md border border-border bg-surface/95 p-2.5 text-xs leading-snug text-text-secondary shadow-lg backdrop-blur"
					role="dialog"
					aria-label="Map asset attribution"
					use:externalLinks
				>
					<p>
						These maps are for marking and navigating your own pins, not a wiki. For
						wiki-style world maps and location data, see
						<a class="text-accent underline decoration-dotted underline-offset-2" href="https://entropianexus.com/maps">Entropia Nexus</a>.
					</p>
				</div>
			{/if}
			<button
				type="button"
				class="rounded-md border border-border bg-base/85 px-2 py-1 text-[10px] text-text-secondary shadow-sm backdrop-blur transition-colors hover:text-text"
				aria-expanded={attributionOpen}
				onclick={() => (attributionOpen = !attributionOpen)}
			>
				Map asset by Entropia Nexus
			</button>
		</div>
	</div>
</div>
