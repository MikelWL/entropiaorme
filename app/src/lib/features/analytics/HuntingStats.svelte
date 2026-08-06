<script lang="ts">
	/**
	 * The Hunting Overall block: the same paired stat grid, tones, and
	 * estimate/realised disclosures as the Tree Cutting headline, extended
	 * with the direct hunting signal (PES, kills, duration, evidence
	 * depth). Full session sustainability, heal, and armour stay on
	 * Dashboard and Overview; this block reports what hunting itself did.
	 */
	import InfoTip from '$lib/components/InfoTip.svelte';
	import StatDisplay from '$lib/components/StatDisplay.svelte';
	import { NO_DATA, formatPed, formatPercent } from '$lib/utils/format';
	import type { HuntingOverallLine } from './huntingModel.svelte';

	let {
		overall,
	}: {
		overall: HuntingOverallLine;
	} = $props();

	const signedPed = (value: number) => `${value >= 0 ? '+' : ''}${formatPed(value)}`;
	const netTone = (value: number) => (value >= 0 ? 'text-positive' : 'text-negative');
	const formatHours = (hours: number) => {
		if (hours >= 10) return `${hours.toFixed(0)}h`;
		if (hours >= 1) return `${hours.toFixed(1)}h`;
		return `${Math.round(hours * 60)}m`;
	};
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

{#snippet directTip()}
	<InfoTip label="What direct figures cover">
		<p class="text-xs font-semibold leading-relaxed text-text">Direct hunting cost only</p>
		<p class="mt-1 text-xs leading-relaxed text-text-secondary">
			Weapon and enhancer decay attributed to kills. Heal and armour are recorded per session,
			not per kill, so they stay in the Dashboard and Overview totals rather than being
			smeared across only some rows here.
		</p>
	</InfoTip>
{/snippet}

<div class="grid grid-cols-[auto_auto] content-start items-end gap-x-10 gap-y-4">
	<h2 class="text-3xl font-bold tracking-tight leading-none text-text">Overall</h2>
	<StatDisplay
		label="Cycled"
		value={formatPed(overall.cycled)}
		unit="PED"
		emphasis="secondary"
		labelSuffix={directTip}
	/>

	<StatDisplay
		label="TT Net"
		value={signedPed(overall.returns - overall.cycled)}
		unit="PED"
	/>
	<StatDisplay label="TT Rate" value={formatPercent(overall.lootRate)} emphasis="secondary" />

	<StatDisplay
		label="MU Net"
		value={overall.muProjectedReturns !== null
			? signedPed(overall.muProjectedReturns - overall.cycled)
			: NO_DATA}
		unit={overall.muProjectedReturns !== null ? 'PED' : ''}
		labelSuffix={estimateTip}
	/>
	<StatDisplay
		label="MU Rate"
		value={overall.muRate !== null ? formatPercent(overall.muRate) : NO_DATA}
		emphasis="secondary"
	/>

	<StatDisplay
		label="Realised Net"
		value={signedPed(overall.realisedReturns - overall.cycled)}
		valueClass={netTone(overall.realisedReturns - overall.cycled)}
		unit="PED"
		labelSuffix={realisedTip}
	/>
	<StatDisplay
		label="Realised Rate"
		value={formatPercent(overall.realisedRate)}
		valueClass={netTone(overall.realisedRate - 1)}
		emphasis="secondary"
	/>

	<StatDisplay label="PES" value={overall.pes.toFixed(2)} />
	<StatDisplay
		label="PES/100"
		value={overall.pesPer100Ped.toFixed(2)}
		emphasis="secondary"
	/>

	<StatDisplay label="Kills" value={String(overall.kills)} emphasis="secondary" />
	<StatDisplay
		label="Duration"
		value={formatHours(overall.durationHours)}
		emphasis="secondary"
	/>

	<StatDisplay
		label="Sessions"
		value={String(overall.sessions)}
		emphasis="secondary"
	/>
</div>
