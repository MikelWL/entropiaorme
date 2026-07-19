<script lang="ts">
	import Button from '$lib/components/Button.svelte';
	import Menu from '$lib/components/Menu.svelte';
	import type { MapPin } from '$lib/api';
	import MapPinPicker from './MapPinPicker.svelte';

	let {
		pins,
		ontoggleoverlay,
		onconfigure,
		oncalibrate,
		onselectpin,
		onroute,
		onradarcalibrate,
	}: {
		pins: MapPin[];
		ontoggleoverlay: () => void;
		onconfigure: () => void;
		oncalibrate: () => void;
		onselectpin: (pin: MapPin) => void;
		onroute: () => void;
		onradarcalibrate: () => void;
	} = $props();
	let searchOpen = $state(false);

	const setupItems = $derived([
		{ label: 'Configure pin overlay', onSelect: onconfigure },
		{ label: 'Calibrate coordinate capture', onSelect: oncalibrate },
		{ label: 'Calibrate radar guidance', onSelect: onradarcalibrate },
		{
			label: searchOpen ? 'Hide pin search' : 'Search pins',
			onSelect: () => (searchOpen = !searchOpen),
		},
	]);
</script>

<div class="relative flex shrink-0 items-center gap-1">
	<Button size="sm" onclick={ontoggleoverlay}>Pin overlay</Button>
	<Button size="sm" variant="secondary" onclick={onroute}>Route</Button>
	<Menu ariaLabel="Map setup" items={setupItems}>
		{#snippet trigger({ open, toggle })}
			<Button size="sm" variant="ghost" aria-haspopup="menu" aria-expanded={open} onclick={toggle}>Setup</Button>
		{/snippet}
	</Menu>

	{#if searchOpen}
		<div class="absolute right-0 top-full z-30 mt-2 w-72 rounded-md border border-border bg-surface-raised p-2 shadow-lg">
			<MapPinPicker {pins} onselect={onselectpin} />
		</div>
	{/if}
</div>
