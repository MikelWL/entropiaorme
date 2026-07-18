<script lang="ts">
	import Button from '$lib/components/Button.svelte';
	import Input from '$lib/components/Input.svelte';
	import Menu from '$lib/components/Menu.svelte';
	import Select from '$lib/components/Select.svelte';
	import type { PlanetMap } from '$lib/api';

	let {
		planets,
		selectedName,
		scanning,
		search = $bindable(''),
		coordinate = $bindable(''),
		showGrid = $bindable(false),
		visiblePins,
		totalPins,
		onselectplanet,
		onscan,
		ontoggleoverlay,
		onconfigure,
		oncalibrate,
		onsearchenter,
		ongoto,
	}: {
		planets: PlanetMap[];
		selectedName: string;
		scanning: boolean;
		search?: string;
		coordinate?: string;
		showGrid?: boolean;
		visiblePins: number;
		totalPins: number;
		onselectplanet: (name: string) => void;
		onscan: () => void;
		ontoggleoverlay: () => void;
		onconfigure: () => void;
		oncalibrate: () => void;
		onsearchenter: () => void;
		ongoto: () => void;
	} = $props();

	const setupItems = $derived([
		{ label: 'Configure pin overlay', onSelect: onconfigure },
		{ label: 'Calibrate coordinate capture', onSelect: oncalibrate },
	]);
</script>

<div class="flex flex-wrap items-end gap-2 rounded-lg border border-border bg-surface/35 p-2">
	<label class="min-w-44 flex-1 space-y-1 sm:max-w-56">
		<span class="text-[11px] text-text-secondary">Planet</span>
		<Select value={selectedName} onchange={(event) => onselectplanet((event.currentTarget as HTMLSelectElement).value)}>
			{#each planets as planet (planet.name)}
				<option value={planet.name}>{planet.name}{planet.calibration ? '' : ' (view-only)'}</option>
			{/each}
		</Select>
	</label>

	<div class="flex items-center gap-1">
		<Button size="sm" loading={scanning} onclick={onscan}>Pin my location</Button>
		<Button size="sm" variant="secondary" onclick={ontoggleoverlay}>Pin overlay</Button>
		<Menu ariaLabel="Map setup" items={setupItems}>
			{#snippet trigger({ open, toggle })}
				<Button size="sm" variant="ghost" aria-haspopup="menu" aria-expanded={open} onclick={toggle}>Setup</Button>
			{/snippet}
		</Menu>
	</div>

	<label class="min-w-44 flex-1 space-y-1 sm:max-w-64">
		<span class="text-[11px] text-text-secondary">Search pins</span>
		<Input
			type="search"
			bind:value={search}
			placeholder="Name or notes"
			onkeydown={(event) => {
				if (event.key === 'Enter') onsearchenter();
			}}
		/>
	</label>
	<span class="self-center whitespace-nowrap text-[11px] tabular-nums text-text-tertiary">
		{visiblePins}/{totalPins}
	</span>

	<form class="flex min-w-64 flex-1 items-end gap-1 sm:max-w-80" onsubmit={(event) => {
		event.preventDefault();
		ongoto();
	}}>
		<label class="min-w-0 flex-1 space-y-1">
			<span class="text-[11px] text-text-secondary">Go to coordinate</span>
			<Input bind:value={coordinate} placeholder="61400, 75800" inputmode="decimal" />
		</label>
		<Button type="submit" size="sm" variant="secondary">Go</Button>
	</form>

	<Button size="sm" variant={showGrid ? 'secondary' : 'ghost'} aria-pressed={showGrid} onclick={() => (showGrid = !showGrid)}>
		Grid
	</Button>
</div>
