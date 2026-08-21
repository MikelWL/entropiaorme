<script lang="ts">
	import InfoTip from '$lib/components/InfoTip.svelte';
	import EffectiveEfficiencyInfoTip from '$lib/components/EffectiveEfficiencyInfoTip.svelte';
	import ExpectedReturnInfoTip from '$lib/components/ExpectedReturnInfoTip.svelte';
	import StatDisplay from '$lib/components/StatDisplay.svelte';
	import type { ExpectedHuntingEconomics } from '$lib/api';
	import { NO_DATA, formatPed, formatPercent } from '$lib/utils/format';
	import type { Snippet } from 'svelte';

	let {
		heading,
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
		headingControl,
	}: {
		heading?: string;
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
		headingControl?: Snippet;
	} = $props();

	const signedPed = (value: number) => `${value >= 0 ? '+' : ''}${formatPed(value)}`;
	const netTone = (value: number) => (value >= 0 ? 'text-positive' : 'text-negative');
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

{#if headingControl}
	<div
		class="grid grid-cols-[minmax(10rem,1.35fr)_repeat(3,minmax(0,1fr))] items-start gap-x-6 gap-y-4"
		data-testid="activity-economic-headline"
	>
		<div class="min-w-0">{@render headingControl()}</div>
		<StatDisplay label="TT Net" value={signedPed(returns - cycled)} unit="PED" />
		<StatDisplay
			label="MU Net"
			value={muProjectedReturns !== null ? signedPed(muProjectedReturns - cycled) : NO_DATA}
			unit={muProjectedReturns !== null ? 'PED' : ''}
			labelSuffix={estimateTip}
		/>
		<StatDisplay
			label="Realised Net"
			value={signedPed(realisedReturns - cycled)}
			valueClass={netTone(realisedReturns - cycled)}
			unit="PED"
			labelSuffix={realisedTip}
		/>

		<div class="border-t border-border/35 pt-3" data-testid="economic-subordinate-cycled">
			<StatDisplay label="Cycled" value={formatPed(cycled)} unit="PED" emphasis="secondary" />
		</div>
		<div class="border-t border-border/35 pt-3" data-testid="economic-subordinate-tt-rate">
			<StatDisplay label="TT Rate" value={formatPercent(lootRate)} valueClass="text-text" emphasis="secondary" />
		</div>
		<div class="border-t border-border/35 pt-3" data-testid="economic-subordinate-mu-rate">
			<StatDisplay
				label="MU Rate"
				value={muRate !== null ? formatPercent(muRate) : NO_DATA}
				valueClass={muRate !== null ? 'text-text' : 'text-text-tertiary'}
				emphasis="secondary"
			/>
		</div>
		<div class="border-t border-border/35 pt-3" data-testid="economic-subordinate-realised-rate">
			<StatDisplay
				label="Realised Rate"
				value={formatPercent(realisedRate)}
				valueClass={netTone(realisedRate - 1)}
				emphasis="secondary"
			/>
		</div>
	</div>
	{#if lootMarkupFactor !== undefined || expectedTtRate !== undefined}
		<div
			class="mt-4 grid grid-cols-2 items-start gap-x-6 gap-y-3 border-t border-border/35 pt-3 md:grid-cols-4"
			data-testid="hunting-expected-economics"
		>
			<StatDisplay
				label="Effective Efficiency"
				value={effectiveEfficiencyValue}
				valueClass={expectedEconomics?.effectiveEfficiency != null
					? 'text-text'
					: 'text-text-tertiary'}
				emphasis="secondary"
				labelSuffix={effectiveEfficiencyTip}
			/>
			<StatDisplay
				label="Loot MU"
				value={lootMarkupFactor != null ? formatPercent(lootMarkupFactor) : NO_DATA}
				valueClass={lootMarkupFactor != null ? 'text-text' : 'text-text-tertiary'}
				emphasis="secondary"
				labelSuffix={estimateTip}
			/>
			<StatDisplay
				label="Expected Return"
				value={expectedTtRate != null ? formatPercent(expectedTtRate) : NO_DATA}
				valueClass={expectedTtRate != null ? 'text-text' : 'text-text-tertiary'}
				emphasis="secondary"
				labelSuffix={expectedReturnTip}
			/>
			<StatDisplay
				label="Expected + MU"
				value={expectedMarketRate != null ? formatPercent(expectedMarketRate) : NO_DATA}
				valueClass={expectedMarketRate != null ? 'text-text' : 'text-text-tertiary'}
				emphasis="secondary"
				labelSuffix={expectedReturnTip}
			/>
		</div>
	{/if}
{:else if heading}
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
