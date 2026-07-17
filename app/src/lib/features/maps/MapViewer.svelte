<script lang="ts">
	/**
	 * The pan/zoom map view: a canvas raster under a DOM pin layer, both
	 * rendered through the one viewport transform (`./viewport`). Wheel
	 * zoom anchors under the cursor; drag pans; a still click on the map
	 * reports the game coordinate for a pin drop; pin markers are real
	 * buttons (hover and keyboard focus both raise the detail card).
	 * Arrow keys pan, +/- zoom about the centre, 0 re-fits.
	 */
	import { untrack } from 'svelte';
	import type { MapPin, PlanetMap } from '$lib/api';
	import { gameToImage, imageToGame, type GamePoint } from './coords';
	import { pinGlyph } from './pinIcons';
	import PinCard from './PinCard.svelte';
	import {
		ZOOM_STEP,
		fitViewport,
		imageToView,
		panBy,
		viewToImage,
		zoomAt,
		type Viewport,
	} from './viewport';

	let {
		planet,
		imageUrl,
		pins,
		onmapclick,
		oncopywaypoint,
		oneditpin,
		ondeletepin,
	}: {
		planet: PlanetMap;
		imageUrl: string;
		pins: MapPin[];
		/** A still click on a calibrated map, in game units. */
		onmapclick: (point: GamePoint) => void;
		oncopywaypoint: (pin: MapPin) => void;
		oneditpin: (pin: MapPin) => void;
		ondeletepin: (pin: MapPin) => void;
	} = $props();

	const cal = $derived(planet.calibration);

	let viewW = $state(0);
	let viewH = $state(0);
	let canvas = $state<HTMLCanvasElement | null>(null);
	let image = $state<HTMLImageElement | null>(null);
	let vp = $state<Viewport>({ zoom: 1, panX: 0, panY: 0 });

	// The active pin card: raised by marker hover or focus, kept open
	// while the pointer is over the card itself, closed on a short delay
	// so travelling marker -> card does not dismiss it.
	let activePin = $state<MapPin | null>(null);
	let closeTimer: ReturnType<typeof setTimeout> | null = null;

	function raiseCard(pin: MapPin) {
		if (closeTimer) clearTimeout(closeTimer);
		activePin = pin;
	}

	function scheduleCardClose() {
		if (closeTimer) clearTimeout(closeTimer);
		closeTimer = setTimeout(() => (activePin = null), 250);
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
		return () => {
			img.onload = null;
			img.onerror = null;
		};
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

	// Draw on dirty: re-runs only when the transform, raster, or size
	// change (no free-running animation loop; the app idles beside a
	// running game).
	$effect(() => {
		const ctx = canvas?.getContext('2d');
		if (!ctx || !canvas || !image || viewW === 0 || viewH === 0) return;
		const dpr = window.devicePixelRatio || 1;
		canvas.width = Math.round(viewW * dpr);
		canvas.height = Math.round(viewH * dpr);
		ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
		ctx.clearRect(0, 0, viewW, viewH);
		ctx.imageSmoothingEnabled = true;
		ctx.imageSmoothingQuality = 'high';
		ctx.drawImage(
			image,
			-vp.panX * vp.zoom,
			-vp.panY * vp.zoom,
			image.naturalWidth * vp.zoom,
			image.naturalHeight * vp.zoom,
		);
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
		(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
		pressed = true;
		dragging = false;
		lastPointer = { x: event.clientX, y: event.clientY };
		pressPointer = lastPointer;
	}

	function handlePointerMove(event: PointerEvent) {
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
		onmapclick(imageToGame(cal, imagePoint));
	}

	const PAN_STEP_PX = 60;
	function handleKeydown(event: KeyboardEvent) {
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
		});
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
	onkeydown={handleKeydown}
>
	<canvas class="absolute inset-0" bind:this={canvas} aria-hidden="true"></canvas>

	<!-- Area-pin discs under the markers. -->
	{#if placedPins.some((placed) => placed.radiusRx != null)}
		<svg
			class="absolute inset-0 h-full w-full pointer-events-none"
			aria-hidden="true"
			viewBox="0 0 {viewW} {viewH}"
		>
			{#each placedPins.filter((placed) => placed.radiusRx != null) as placed (placed.pin.id)}
				<ellipse
					cx={placed.x}
					cy={placed.y}
					rx={placed.radiusRx}
					ry={placed.radiusRy}
					class="fill-accent/10 stroke-accent/50"
					stroke-width="1.5"
				/>
			{/each}
		</svg>
	{/if}

	<!-- Pin markers: real buttons, so hover and keyboard focus share one
	     detail surface. -->
	{#each placedPins as placed (placed.pin.id)}
		<button
			type="button"
			class="absolute -translate-x-1/2 -translate-y-full cursor-pointer text-xl leading-none drop-shadow-md transition-transform hover:scale-125 focus-visible:scale-125 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent rounded-sm"
			style="left: {placed.x}px; top: {placed.y}px;"
			aria-label="Pin: {placed.pin.name}"
			onmouseenter={() => raiseCard(placed.pin)}
			onmouseleave={scheduleCardClose}
			onfocus={() => raiseCard(placed.pin)}
			onblur={scheduleCardClose}
			onpointerdown={(event) => event.stopPropagation()}
			onclick={(event) => {
				event.stopPropagation();
				raiseCard(placed.pin);
			}}
		>
			{pinGlyph(placed.pin.icon)}
		</button>
		<!-- The active card renders immediately after its marker, so Tab
		     moves from the marker straight into the card's actions; focus
		     inside the card holds it open exactly like pointer hover. -->
		{#if activePin?.id === placed.pin.id}
			<PinCard
				pin={placed.pin}
				x={placed.x}
				y={placed.y}
				{viewW}
				{viewH}
				technicalName={planet.technicalName}
				onpointerenter={() => raiseCard(placed.pin)}
				onpointerleave={scheduleCardClose}
				oncopywaypoint={() => oncopywaypoint(placed.pin)}
				onedit={() => oneditpin(placed.pin)}
				ondelete={() => {
					activePin = null;
					ondeletepin(placed.pin);
				}}
			/>
		{/if}
	{/each}

	{#if !cal}
		<p
			class="absolute bottom-3 left-1/2 -translate-x-1/2 rounded-md bg-surface/90 px-3 py-1.5 text-xs text-text-secondary"
		>
			This map has no coordinate calibration yet: it is view-only, so pins cannot be placed.
		</p>
	{/if}
</div>
