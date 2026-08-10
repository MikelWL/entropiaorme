<script lang="ts">
	import SegmentedControl from '$lib/components/SegmentedControl.svelte';
	import StatDisplay from '$lib/components/StatDisplay.svelte';
	import { formatPed } from '$lib/utils/format';
	import type { InventoryKind, InventoryView } from './inventoryModel.svelte';

	let {
		kind,
		view,
		heldValue,
		distinctHoldings,
		costBasis,
		openListings,
		onviewchange,
	}: {
		kind: InventoryKind;
		view: InventoryView;
		heldValue: number;
		distinctHoldings: number;
		costBasis: number;
		openListings: number;
		onviewchange: (id: string) => void;
	} = $props();

	const viewOptions = [
		{ id: 'holdings', label: 'Holdings' },
		{ id: 'listings', label: 'Listings' },
		{ id: 'history', label: 'History' },
	];
</script>

<section
	aria-label="Inventory summary"
	class="grid grid-cols-2 items-start gap-x-6 gap-y-5 border-b border-border/50 pb-5 sm:grid-cols-[minmax(10rem,1.25fr)_repeat(3,minmax(0,1fr))]"
>
	<div class="col-span-2 min-w-0 sm:col-span-1">
		<p class="eyebrow mb-2 text-text-tertiary">Workspace</p>
		<SegmentedControl options={viewOptions} active={view} onchange={onviewchange} />
	</div>
	<StatDisplay label="Held value" value={formatPed(heldValue)} unit="PED" />
	<StatDisplay
		label={kind === 'loot' ? 'Distinct holdings' : 'Cost basis'}
		value={kind === 'loot' ? distinctHoldings : formatPed(costBasis)}
		unit={kind === 'equipment' ? 'PED' : ''}
	/>
	<StatDisplay label="Open listings" value={openListings} />
</section>
