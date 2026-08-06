<script lang="ts">
	/**
	 * The Hunting Overall block: the same paired stat grid, tones, and
	 * estimate/realised disclosures as the Tree Cutting headline. The
	 * pairing rule holds throughout: left column is a primary net figure,
	 * right column its secondary rate. The evidence depth (sessions, kills,
	 * duration) lives in a compact strip under the heading rather than as
	 * grid rows, so the block stays the same height family as its sibling.
	 * Full session sustainability, heal, and armour stay on Dashboard and
	 * Overview; this block reports what hunting itself did.
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
		{#if overall.realisedOutsidePeriod > 0.005}
			<p class="mt-2 text-xs leading-relaxed text-text-tertiary">
				{formatPed(overall.realisedOutsidePeriod)} PED of the confirmed markup belongs to
				species not hunted in the selected period. It is still counted here, because the sale
				happened whichever period is showing.
			</p>
		{/if}
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

{#snippet pesTip()}
	<InfoTip label="What PES is">
		<p class="text-xs font-semibold leading-relaxed text-text">Skill progress, not money</p>
		<p class="mt-1 text-xs leading-relaxed text-text-secondary">
			Project Entropia Skill: the non-liquid denomination of skill progress, derived from the
			skill curve. It never enters a PED figure; PES/100 is PES earned per 100 PED cycled,
			the primary skilling comparison.
		</p>
	</InfoTip>
{/snippet}

<div class="grid grid-cols-[auto_auto] content-start items-end gap-x-10 gap-y-4">
	<div class="min-w-0">
		<h2 class="text-3xl font-bold tracking-tight leading-none text-text">Overall</h2>
		<!-- The evidence strip: how much play stands behind the figures. -->
		<p class="mt-1.5 whitespace-nowrap text-xs tabular-nums text-text-tertiary">
			{overall.sessions}
			{overall.sessions === 1 ? 'session' : 'sessions'} · {overall.kills}
			{overall.kills === 1 ? 'kill' : 'kills'} · {formatHours(overall.durationHours)}
		</p>
	</div>
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
		labelSuffix={realisedTip}
	/>

	<StatDisplay label="PES" value={overall.pes.toFixed(2)} labelSuffix={pesTip} />
	<StatDisplay
		label="PES/100"
		value={overall.pesPer100Ped.toFixed(2)}
		emphasis="secondary"
	/>
</div>
