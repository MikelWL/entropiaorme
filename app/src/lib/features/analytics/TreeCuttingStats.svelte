<script lang="ts">
	import InfoTip from '$lib/components/InfoTip.svelte';
	import EffectiveEfficiencyInfoTip from '$lib/components/EffectiveEfficiencyInfoTip.svelte';
	import ExpectedReturnInfoTip from '$lib/components/ExpectedReturnInfoTip.svelte';
	import RewardMuInfoTip from '$lib/components/RewardMuInfoTip.svelte';
	import type { ExpectedHuntingEconomics } from '$lib/api';
	import { NO_DATA } from '$lib/utils/format';
	import CompletionRewardContext from './CompletionRewardContext.svelte';
	import EconomicOutcomeHorizon from './EconomicOutcomeHorizon.svelte';
	import ExpectedEconomicsEquation from './ExpectedEconomicsEquation.svelte';
	import type { RewardContext } from './huntingModel.svelte';

	let {
		cycled,
		returns,
		lootRate,
		muProjectedReturns,
		muRate,
		lootMarkupFactor,
		expectedTtRate,
		expectedMarketRate,
		expectedEconomics,
		realisedReturns,
		realisedRate,
		reward = null,
		rewardMuRate = null,
		expectedTotalRate = null,
	}: {
		cycled: number;
		returns: number;
		lootRate: number;
		muProjectedReturns: number | null;
		muRate: number | null;
		lootMarkupFactor?: number | null;
		expectedTtRate?: number | null;
		expectedMarketRate?: number | null;
		expectedEconomics?: ExpectedHuntingEconomics | null;
		realisedReturns: number;
		realisedRate: number;
		/** Completion rewards aggregated over the scope's activities. Absent
		 * for activities that do not have them at all, Tree Cutting included. */
		reward?: RewardContext | null;
		rewardMuRate?: number | null;
		expectedTotalRate?: number | null;
	} = $props();

	const effectiveEfficiencyValue = $derived.by(() => {
		const effective = expectedEconomics?.effectiveEfficiency;
		if (!effective) return NO_DATA;
		switch (effective.status) {
			case 'within_model_range':
				return effective.efficiencyPct !== null
					? `${effective.efficiencyPct.toFixed(1)}%`
					: NO_DATA;
			case 'below_model_range':
				return 'Below model range';
			case 'above_model_range':
				return 'Above model range';
		}
	});
</script>

{#snippet estimateTip()}
	<InfoTip label="What MU figures are">
		<p class="text-xs font-semibold leading-relaxed text-text">Estimated, not realised</p>
		<p class="mt-1 text-xs leading-relaxed text-text-secondary">
			What current market markup would add if this loot sold at it. Nothing here is money
			until a sale is confirmed.
		</p>
	</InfoTip>
{/snippet}

{#snippet expectedReturnTip()}
	<ExpectedReturnInfoTip
		looterLevel={expectedEconomics?.looterLevel}
		coverage={expectedEconomics?.coverage}
		incomplete={expectedEconomics?.incomplete}
	/>
{/snippet}

{#snippet effectiveEfficiencyTip()}
	<EffectiveEfficiencyInfoTip
		effectiveEfficiency={expectedEconomics?.effectiveEfficiency ?? null}
		looterLevel={expectedEconomics?.looterLevel}
		coverage={expectedEconomics?.coverage}
		incomplete={expectedEconomics?.incomplete}
		scope="activity"
	/>
{/snippet}

{#snippet realisedTip()}
	<InfoTip label="What realised figures are">
		<p class="text-xs font-semibold leading-relaxed text-text">Loot TT plus realised markup</p>
		<p class="mt-1 text-xs leading-relaxed text-text-secondary">
			Only markup from a recorded sale or deliberate Shrapnel conversion. It reads the same as
			TT Net until a stock outcome realises a gain or loss.
		</p>
	</InfoTip>
{/snippet}

{#snippet rewardMuTip()}
	<RewardMuInfoTip />
{/snippet}

<EconomicOutcomeHorizon
	{cycled}
	ttNet={returns - cycled}
	ttRate={lootRate}
	muNet={muProjectedReturns !== null ? muProjectedReturns - cycled : null}
	{muRate}
	realisedNet={realisedReturns - cycled}
	{realisedRate}
	muTip={estimateTip}
	realisedTip={realisedTip}
/>

{#if reward && reward.treatments.length > 0}
	<div class="mt-5">
		<CompletionRewardContext
			ttPed={reward.rewardTtPed}
			muPed={reward.rewardMuPed}
			treatments={reward.treatments}
			scope="session"
		/>
	</div>
{/if}

{#if lootMarkupFactor !== undefined || expectedTtRate !== undefined}
	<ExpectedEconomicsEquation
		effectiveEfficiency={effectiveEfficiencyValue}
		lootMarkupFactor={lootMarkupFactor ?? null}
		expectedTtRate={expectedTtRate ?? null}
		expectedMarketRate={expectedMarketRate ?? null}
		{rewardMuRate}
		{expectedTotalRate}
		efficiencyTip={effectiveEfficiencyTip}
		estimateTip={estimateTip}
		expectedTip={expectedReturnTip}
		rewardTip={rewardMuTip}
	/>
{/if}
