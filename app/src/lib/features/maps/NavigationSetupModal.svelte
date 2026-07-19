<script lang="ts">
	import Button from '$lib/components/Button.svelte';
	import Modal from '$lib/components/Modal.svelte';
	import Select from '$lib/components/Select.svelte';
	import { scanMapCoordinates, startNavigation, type MapPin, type NavigationRun } from '$lib/api';
	import { describeError } from '$lib/view/errorState';
	import { formatGamePoint, type GamePoint } from './coords';

	let {
		open = $bindable(false),
		planet,
		mapViewId,
		pins,
		onstarted,
	}: {
		open?: boolean;
		planet: string;
		mapViewId: number | null;
		pins: MapPin[];
		onstarted: (run: NavigationRun) => void;
	} = $props();

	let start = $state<GamePoint | null>(null);
	let hops = $state(25);
	let hotkey = $state('f8');
	let busy = $state(false);
	let error = $state<string | null>(null);

	$effect(() => {
		if (!open) {
			start = null;
			error = null;
		}
	});

	async function captureStart() {
		busy = true;
		error = null;
		try {
			const result = await scanMapCoordinates(planet);
			if (result.status !== 'read' || result.lon == null || result.lat == null) {
				error = result.status === 'noRegion'
					? 'Calibrate coordinate capture before starting a route.'
					: 'The current position could not be read. Keep the coordinate readout visible and try again.';
				return;
			}
			start = { lon: result.lon, lat: result.lat };
		} catch (cause) {
			error = describeError(cause, 'The current position could not be captured');
		} finally {
			busy = false;
		}
	}

	async function beginRoute() {
		if (!start) return;
		busy = true;
		error = null;
		try {
			const run = await startNavigation(planet, mapViewId, start.lon, start.lat, hops, hotkey);
			onstarted(run);
			open = false;
		} catch (cause) {
			error = describeError(cause, 'The route could not be created');
		} finally {
			busy = false;
		}
	}
</script>

<Modal bind:open title="Plan route">
	<div class="space-y-4 text-sm">
		<div class="rounded-md border border-border bg-base/60 p-3">
			<p class="text-xs uppercase tracking-wide text-text-tertiary">Starting position</p>
			<p class="mt-1 tabular-nums text-text">{start ? formatGamePoint(start) : 'Not captured'}</p>
			<Button class="mt-3" size="sm" variant="secondary" disabled={busy} onclick={captureStart}>
				{start ? 'Capture again' : 'Capture current position'}
			</Button>
			<div class="mt-2">
				<Select aria-label="Use a pin as route start" value="" onchange={(event) => {
					const id = Number((event.currentTarget as HTMLSelectElement).value);
					const pin = pins.find((candidate) => candidate.id === id);
					if (pin) start = { lon: pin.lon, lat: pin.lat };
				}}>
					<option value="">Or choose a pin…</option>
					{#each pins as pin (pin.id)}<option value={pin.id}>{pin.name}</option>{/each}
				</Select>
			</div>
		</div>
		<label class="block">
			<span class="mb-1 block text-xs text-text-secondary">Stops</span>
			<input class="field w-full" type="number" min="1" max="500" bind:value={hops} />
		</label>
		<label class="block">
			<span class="mb-1 block text-xs text-text-secondary">Update hotkey</span>
			<Select bind:value={hotkey} aria-label="Navigation update hotkey">
				{#each ['f6', 'f7', 'f8', 'f9', 'f10', 'f11', 'f12'] as key}
					<option value={key}>{key.toUpperCase()}</option>
				{/each}
			</Select>
		</label>
		<p class="text-xs text-text-secondary">
			The route starts here, visits up to {hops} pins efficiently, and does not return to the start.
		</p>
		{#if error}<p class="text-xs text-danger" role="alert">{error}</p>{/if}
		<div class="flex justify-end gap-2">
			<Button variant="ghost" onclick={() => (open = false)}>Cancel</Button>
			<Button disabled={busy || !start || hops < 1 || hops > 500} onclick={beginRoute}>Start route</Button>
		</div>
	</div>
</Modal>
