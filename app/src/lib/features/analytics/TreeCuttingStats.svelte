<script lang="ts">
	import StatDisplay from '$lib/components/StatDisplay.svelte';
	import { formatPed, formatPercent } from '$lib/utils/format';

	let {
		heading,
		cycled,
		swings,
		returns,
		lootRate,
		marketReturns,
		marketRate,
		realisedReturns,
		realisedRate,
	}: {
		heading?: string;
		cycled: number;
		swings?: number;
		returns: number;
		lootRate: number;
		marketReturns: number | null;
		marketRate: number | null;
		realisedReturns: number;
		realisedRate: number;
	} = $props();

	const NO_DATA = 'N/A';
	const signedPed = (value: number) => `${value >= 0 ? '+' : ''}${formatPed(value)}`;
	const netTone = (value: number) => (value >= 0 ? 'text-positive' : 'text-negative');
</script>

<div class="grid grid-cols-[auto_auto] content-start items-end gap-x-10 gap-y-4">
	{#if heading}
		<h2 class="text-3xl font-bold tracking-tight leading-none text-text">{heading}</h2>
	{:else}
		<StatDisplay label="Cycled" value={formatPed(cycled)} unit="PED" />
	{/if}

	{#if heading}
		<StatDisplay label="Cycled" value={formatPed(cycled)} unit="PED" />
	{:else if swings !== undefined}
		<StatDisplay label="Swings" value={swings.toLocaleString()} />
	{:else}
		<span></span>
	{/if}

	<StatDisplay
		label="TT Floor Net"
		value={signedPed(returns - cycled)}
		valueClass={netTone(returns - cycled)}
		unit="PED"
	/>
	<StatDisplay label="TT Floor Rate" value={formatPercent(lootRate)} />

	<StatDisplay
		label="Current Market Net"
		value={marketReturns !== null ? signedPed(marketReturns - cycled) : NO_DATA}
		unit={marketReturns !== null ? 'PED' : ''}
	/>
	<StatDisplay
		label="Current Market Rate"
		value={marketRate !== null ? formatPercent(marketRate) : NO_DATA}
	/>

	<StatDisplay
		label="Realised Net"
		value={signedPed(realisedReturns - cycled)}
		valueClass={netTone(realisedReturns - cycled)}
		unit="PED"
	/>
	<StatDisplay label="Realised Rate" value={formatPercent(realisedRate)} />
</div>
