<script lang="ts">
	import { onMount } from 'svelte';
	import { getCurrentWindow } from '@tauri-apps/api/window';
	import { listen, emit } from '@tauri-apps/api/event';
	import {
		createMapPin,
		getMapViews,
		getPlanetMaps,
		scanMapCoordinates,
		type MapView,
		type PlanetMap,
	} from '$lib/api';
	import { describeError } from '$lib/view/errorState';
	import { createWindowSizeSync } from '$lib/windows/windowSize';
	import { pinGlyph } from '$lib/features/maps/pinIcons';
	import {
		acceptCartographyOverlayBroadcast,
		cartographyPinInput,
		cartographyScanFailureMessage,
		cartographyOverlayConfig,
		CARTOGRAPHY_OVERLAY_CHANGED_EVENT,
		initCartographyOverlay,
		MAP_PINS_CHANGED_EVENT,
		type CartographyButton,
	} from '$lib/features/maps/cartographyOverlay.svelte';

	let root: HTMLDivElement;
	const sizeSync = createWindowSizeSync(() => root);
	let planets = $state<PlanetMap[]>([]);
	let views = $state<MapView[]>([]);
	const calibratedPlanets = $derived(planets.filter((planet) => planet.calibration !== null));
	const selectedMapName = $derived(
		cartographyOverlayConfig.current.mapViewId === null
			? 'Default'
			: views.find((view) => view.id === cartographyOverlayConfig.current.mapViewId)?.name ??
				'Selected map',
	);
	let busy = $state(false);
	let feedback = $state<{ text: string; success: boolean } | null>(null);
	let feedbackTimer: ReturnType<typeof setTimeout> | null = null;
	let viewsRefreshEpoch = 0;

	function flash(text: string, success = false) {
		feedback = { text, success };
		if (feedbackTimer) clearTimeout(feedbackTimer);
		feedbackTimer = setTimeout(() => (feedback = null), 3500);
	}

	async function refreshViews(): Promise<void> {
		const planet = cartographyOverlayConfig.current.planet;
		const epoch = ++viewsRefreshEpoch;
		if (!planet) {
			views = [];
			return;
		}
		try {
			const loadedViews = await getMapViews(planet);
			if (
				epoch === viewsRefreshEpoch &&
				planet === cartographyOverlayConfig.current.planet
			) {
				views = loadedViews;
			}
		} catch {
			// Preserve the last-good view list; a later event can restore live state.
		}
	}

	onMount(() => {
		let mounted = true;
		let unlisten: (() => void) | undefined;
		void (async () => {
			await initCartographyOverlay();
			if (!mounted) return;
			const loadedPlanets = await getPlanetMaps();
			if (!mounted) return;
			planets = loadedPlanets;
			await refreshViews();
			if (!mounted) return;
			const stopListening = await listen(CARTOGRAPHY_OVERLAY_CHANGED_EVENT, (event) => {
				acceptCartographyOverlayBroadcast(event.payload);
				void refreshViews();
			});
			if (mounted) unlisten = stopListening;
			else stopListening();
		})();
		sizeSync.schedule();
		const observer = new ResizeObserver(() => sizeSync.schedule());
		observer.observe(root);
		return () => {
			mounted = false;
			viewsRefreshEpoch++;
			unlisten?.();
			observer.disconnect();
			sizeSync.cancel();
			if (feedbackTimer) clearTimeout(feedbackTimer);
		};
	});

	function startDraggingFromSurface(event: PointerEvent) {
		if (event.button !== 0 || !(event.target instanceof Element)) return;
		if (event.target.closest('button, select, input, textarea, a, [role="button"]')) return;
		void getCurrentWindow().startDragging();
	}

	async function dropPin(button: CartographyButton) {
		const planet = cartographyOverlayConfig.current.planet;
		const mapViewId = cartographyOverlayConfig.current.mapViewId;
		if (busy) return;
		if (!planet || !calibratedPlanets.some((candidate) => candidate.name === planet)) {
			flash('Choose a calibrated planet in Maps first.');
			return;
		}
		busy = true;
		try {
			const result = await scanMapCoordinates(planet);
			const input = cartographyPinInput(
				planet,
				mapViewId,
				button,
				result,
			);
			if (!input) {
				flash(cartographyScanFailureMessage(result.status, planet));
				return;
			}
			await createMapPin(input);
			void emit(MAP_PINS_CHANGED_EVENT, { planet });
			flash(`${button.name} pinned.`, true);
		} catch (error) {
			flash(describeError(error, 'The pin could not be saved'));
		} finally {
			busy = false;
		}
	}
</script>

<!-- The window surface is a drag handle except where an interactive control owns the pointer. -->
<!-- svelte-ignore a11y_no_static_element_interactions -->

<div bind:this={root} class="overlay-frame flex w-max items-start p-2" onpointerdown={startDraggingFromSurface}>
	<div class="glass-panel overlay-strip flex w-max items-center gap-3 rounded-xl px-4 py-2">
		<div
			class="flex min-w-0 max-w-48 shrink-0 flex-col justify-center border-r border-white/10 pr-3"
			title={`${cartographyOverlayConfig.current.planet ?? 'No planet'} · ${selectedMapName}`}
		>
			<span class="text-[9px] font-bold uppercase leading-none tracking-wider text-white/35">
				Pinning to
			</span>
			<span class="mt-1 truncate text-[11px] font-medium leading-none text-white/70">
				{cartographyOverlayConfig.current.planet ?? 'No planet'} · {selectedMapName}
			</span>
		</div>

		<div class="flex shrink-0 items-center gap-1.5">
			{#each cartographyOverlayConfig.current.buttons as button (button.id)}
				<button
					class="pin-button"
					disabled={busy || !cartographyOverlayConfig.current.planet}
					onclick={() => dropPin(button)}
				>
					<span aria-hidden="true">{pinGlyph(button.icon)}</span>
					<span>{button.name}</span>
				</button>
			{/each}
		</div>

		{#if feedback}
			<p
				role="status"
				class="max-w-48 shrink-0 border-l border-white/10 pl-3 text-[10px] font-medium leading-snug {feedback.success
					? 'text-emerald-300/90'
					: 'text-orange-300/90'}"
			>
				{feedback.text}
			</p>
		{/if}

		<button
			class="release-btn shrink-0"
			aria-label="Hide pin overlay"
			onclick={() => getCurrentWindow().hide()}
		>×</button>
	</div>
</div>

<style>
	.overlay-frame,
	.overlay-strip {
		overflow: visible;
	}

	.glass-panel {
		background: rgba(10, 14, 23, 0.85);
		backdrop-filter: blur(16px) saturate(150%);
		border: 1px solid rgba(255, 255, 255, 0.08);
	}

	.pin-button {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 6px;
		min-width: 76px;
		padding: 4px 9px;
		border-radius: 5px;
		border: 1px solid rgba(255, 255, 255, 0.1);
		background: rgba(255, 255, 255, 0.05);
		color: rgba(255, 255, 255, 0.7);
		font-size: 11px;
		font-weight: 500;
		line-height: 1.25;
		cursor: pointer;
		transition: background 150ms ease-out, border-color 150ms ease-out, color 150ms ease-out;
	}
	.pin-button:hover {
		background: rgba(56, 189, 248, 0.12);
		border-color: rgba(56, 189, 248, 0.35);
		color: rgba(125, 211, 252, 0.95);
	}
	.pin-button:disabled {
		opacity: 0.4;
		cursor: default;
	}

	.release-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 18px;
		height: 18px;
		border-radius: 4px;
		border: 1px solid rgba(255, 255, 255, 0.15);
		background: rgba(255, 255, 255, 0.05);
		color: rgba(255, 255, 255, 0.4);
		font-size: 12px;
		line-height: 1;
		cursor: pointer;
		transition: all 150ms ease-out;
	}
	.release-btn:hover {
		background: rgba(255, 255, 255, 0.1);
		border-color: rgba(255, 255, 255, 0.25);
		color: rgba(255, 255, 255, 0.7);
	}
</style>
