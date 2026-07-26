<script lang="ts">
	import InfoTip from '$lib/components/InfoTip.svelte';
	import StatDisplay from '$lib/components/StatDisplay.svelte';
	import { NO_DATA, formatPed, formatPercent } from '$lib/utils/format';

	let {
		heading,
		cycled,
		returns,
		lootRate,
		muProjectedReturns,
		muRate,
		realisedReturns,
		realisedRate,
	}: {
		heading?: string;
		cycled: number;
		returns: number;
		lootRate: number;
		muProjectedReturns: number | null;
		muRate: number | null;
		realisedReturns: number;
		realisedRate: number;
	} = $props();

	const signedPed = (value: number) => `${value >= 0 ? '+' : ''}${formatPed(value)}`;
	const netTone = (value: number) => (value >= 0 ? 'text-positive' : 'text-negative');
</script>

<!-- An estimate sits here at the same weight as a measured figure, so it says
	so where it is read. Market markup is not money until a sale confirms it,
	and a headline number is exactly where that gets forgotten. -->
{#snippet estimateTip()}
	<InfoTip label="What MU figures are">
		<p class="text-xs font-semibold leading-relaxed text-text">Estimated, not realised</p>
		<p class="mt-1 text-xs leading-relaxed text-text-secondary">
			What current market markup would add if this loot sold at it. Nothing here is money
			until a sale is confirmed.
		</p>
	</InfoTip>
{/snippet}

{#snippet realisedTip()}
	<InfoTip label="What realised figures are">
		<p class="text-xs font-semibold leading-relaxed text-text">Loot TT plus confirmed markup</p>
		<p class="mt-1 text-xs leading-relaxed text-text-secondary">
			Only markup that a confirmed sale actually produced. It reads the same as TT Net until
			something sells.
		</p>
	</InfoTip>
{/snippet}

{#if heading}
	<div class="grid grid-cols-[auto_auto] content-start items-end gap-x-10 gap-y-4">
		<h2 class="text-3xl font-bold tracking-tight leading-none text-text">{heading}</h2>
		<StatDisplay label="Cycled" value={formatPed(cycled)} unit="PED" emphasis="secondary" />

		<StatDisplay
			label="TT Net"
			value={signedPed(returns - cycled)}
			unit="PED"
		/>
		<StatDisplay label="TT Rate" value={formatPercent(lootRate)} emphasis="secondary" />

		<StatDisplay
			label="MU Net"
			value={muProjectedReturns !== null ? signedPed(muProjectedReturns - cycled) : NO_DATA}
			unit={muProjectedReturns !== null ? 'PED' : ''}
			labelSuffix={estimateTip}
		/>
		<StatDisplay
			label="MU Rate"
			value={muRate !== null ? formatPercent(muRate) : NO_DATA}
			emphasis="secondary"
		/>

		<StatDisplay
			label="Realised Net"
			value={signedPed(realisedReturns - cycled)}
			valueClass={netTone(realisedReturns - cycled)}
			unit="PED"
			labelSuffix={realisedTip}
		/>
		<StatDisplay
			label="Realised Rate"
			value={formatPercent(realisedRate)}
			valueClass={netTone(realisedRate - 1)}
			emphasis="secondary"
		/>
	</div>
{:else}
	<div class="grid grid-cols-2 gap-x-4 gap-y-4 sm:grid-cols-3">
		<StatDisplay label="Cycled" value={formatPed(cycled)} unit="PED" />
		<StatDisplay
			label="TT Net"
			value={signedPed(returns - cycled)}
			unit="PED"
		/>
		<StatDisplay label="TT Rate" value={formatPercent(lootRate)} />

		<StatDisplay
			label="MU Net"
			value={muProjectedReturns !== null ? signedPed(muProjectedReturns - cycled) : NO_DATA}
			unit={muProjectedReturns !== null ? 'PED' : ''}
			labelSuffix={estimateTip}
		/>
		<StatDisplay
			label="MU Rate"
			value={muRate !== null ? formatPercent(muRate) : NO_DATA}
		/>
	</div>
{/if}
