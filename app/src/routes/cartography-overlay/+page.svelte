<script lang="ts">
	import { onMount } from 'svelte';
	import { getCurrentWindow } from '@tauri-apps/api/window';
	import { listen, emit } from '@tauri-apps/api/event';
	import { createMapPin, getPlanetMaps, scanMapCoordinates, type PlanetMap } from '$lib/api';
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
		setCartographyOverlayConfig,
		type CartographyButton,
	} from '$lib/features/maps/cartographyOverlay.svelte';

	let root: HTMLDivElement;
	const sizeSync = createWindowSizeSync(() => root);
	let planets = $state<PlanetMap[]>([]);
	const calibratedPlanets = $derived(planets.filter((planet) => planet.calibration !== null));
	let busy = $state(false);
	let feedback = $state<{ text: string; success: boolean } | null>(null);
	let feedbackTimer: ReturnType<typeof setTimeout> | null = null;

	function flash(text: string, success = false) {
		feedback = { text, success };
		if (feedbackTimer) clearTimeout(feedbackTimer);
		feedbackTimer = setTimeout(() => (feedback = null), 3500);
	}

	async function ensureKnownPlanet(): Promise<void> {
		const selected = cartographyOverlayConfig.current.planet;
		if (calibratedPlanets.some((planet) => planet.name === selected)) return;
		await setCartographyOverlayConfig({
			...cartographyOverlayConfig.current,
			planet: calibratedPlanets[0]?.name ?? null,
		});
	}

	onMount(() => {
		let unlisten: (() => void) | undefined;
		void (async () => {
			await initCartographyOverlay();
			planets = await getPlanetMaps();
			await ensureKnownPlanet();
			unlisten = await listen(CARTOGRAPHY_OVERLAY_CHANGED_EVENT, (event) => {
				acceptCartographyOverlayBroadcast(event.payload);
				void ensureKnownPlanet();
			});
		})();
		sizeSync.schedule();
		const observer = new ResizeObserver(() => sizeSync.schedule());
		observer.observe(root);
		return () => {
			unlisten?.();
			observer.disconnect();
			sizeSync.cancel();
			if (feedbackTimer) clearTimeout(feedbackTimer);
		};
	});

	async function selectPlanet(event: Event) {
		await setCartographyOverlayConfig({
			...cartographyOverlayConfig.current,
			planet: (event.currentTarget as HTMLSelectElement).value,
		});
	}

	function startDraggingFromSurface(event: PointerEvent) {
		if (event.button !== 0 || !(event.target instanceof Element)) return;
		if (event.target.closest('button, select, input, textarea, a, [role="button"]')) return;
		void getCurrentWindow().startDragging();
	}

	async function dropPin(button: CartographyButton) {
		const planet = cartographyOverlayConfig.current.planet;
		if (busy) return;
		if (!planet || !calibratedPlanets.some((candidate) => candidate.name === planet)) {
			flash('Choose a calibrated planet in Maps first.');
			return;
		}
		busy = true;
		try {
			const result = await scanMapCoordinates(planet);
			const input = cartographyPinInput(planet, button, result);
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
<div bind:this={root} class="p-1.5" onpointerdown={startDraggingFromSurface}>
	<div class="overflow-hidden rounded-lg border border-border/80 bg-base/95 shadow-xl backdrop-blur-md">
		<div class="flex items-center gap-2 border-b border-border/70 px-2 py-1.5" data-tauri-drag-region>
			<select
				aria-label="Planet"
				class="min-w-0 flex-1 bg-transparent text-xs font-medium text-text outline-none"
				value={cartographyOverlayConfig.current.planet ?? ''}
				onchange={selectPlanet}
			>
				{#each calibratedPlanets as planet (planet.name)}
					<option value={planet.name}>{planet.name}</option>
				{/each}
			</select>
			<button
				class="rounded px-1.5 text-sm text-text-secondary hover:bg-surface hover:text-text"
				aria-label="Hide pin overlay"
				onclick={() => getCurrentWindow().hide()}
			>×</button>
		</div>
		<div class="flex flex-wrap gap-1.5 p-2">
			{#each cartographyOverlayConfig.current.buttons as button (button.id)}
				<button
					class="flex min-w-[5.5rem] flex-1 items-center justify-center gap-1.5 rounded-md border border-border bg-surface/70 px-2 py-2 text-xs font-medium text-text transition hover:border-accent/60 hover:bg-accent/10 disabled:opacity-50"
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
				class="border-t px-2 py-1.5 text-center text-xs {feedback.success
					? 'border-success/40 bg-success/10 text-success'
					: 'border-danger/30 text-text-secondary'}"
			>
				{feedback.text}
			</p>
		{/if}
	</div>
</div>
