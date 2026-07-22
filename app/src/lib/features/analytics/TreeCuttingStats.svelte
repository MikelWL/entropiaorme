<script lang="ts">
	import InfoTip from '$lib/components/InfoTip.svelte';
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

<div class="space-y-5">
	<div class="flex flex-wrap items-end justify-between gap-4">
		{#if heading}
			<h2 class="text-3xl font-bold tracking-tight leading-none text-text">{heading}</h2>
		{/if}
		<div class="flex flex-wrap items-end gap-8 {heading ? '' : 'w-full'}">
			<StatDisplay label="Cycled" value={formatPed(cycled)} unit="PED" />
			{#if swings !== undefined}
				<StatDisplay label="Swings" value={swings.toLocaleString()} />
			{/if}
		</div>
	</div>

	<div class="grid gap-5 sm:grid-cols-3">
		<section class="min-w-0 space-y-3 border-t border-border/50 pt-3">
			<div class="flex items-center gap-2">
				<span class="eyebrow">TT floor</span>
				<InfoTip label="What TT floor means">
					<p class="text-xs leading-relaxed text-text-secondary">
						Loot at its Trade Terminal value. It is liquid bankroll and does not assume any
						market sale.
					</p>
				</InfoTip>
			</div>
			<div class="grid grid-cols-2 gap-4">
				<StatDisplay
					label="Net"
					value={signedPed(returns - cycled)}
					valueClass={netTone(returns - cycled)}
					unit="PED"
				/>
				<StatDisplay label="Rate" value={formatPercent(lootRate)} />
			</div>
		</section>

		<section class="min-w-0 space-y-3 border-t border-accent/50 pt-3">
			<div class="flex items-center gap-2">
				<span class="eyebrow text-accent">Current market</span>
				<InfoTip label="What current market means">
					<div class="space-y-1.5 text-xs leading-relaxed text-text-secondary">
						<p class="text-text">
							What today's market implies for repeating this observed loot composition.
						</p>
						<p>
							It uses markup, turnover, evidence horizon, and fee-efficient parcel size. It
							does not use your current stock and is not realised P&amp;L.
						</p>
					</div>
				</InfoTip>
			</div>
			<div class="grid grid-cols-2 gap-4">
				<StatDisplay
					label="Net"
					value={marketReturns !== null ? signedPed(marketReturns - cycled) : NO_DATA}
					unit={marketReturns !== null ? 'PED' : ''}
				/>
				<StatDisplay
					label="Rate"
					value={marketRate !== null ? formatPercent(marketRate) : NO_DATA}
				/>
			</div>
		</section>

		<section class="min-w-0 space-y-3 border-t border-positive/50 pt-3">
			<div class="flex items-center gap-2">
				<span class="eyebrow text-positive">Realised</span>
				<InfoTip align="right" label="What realised means">
					<div class="space-y-1.5 text-xs leading-relaxed text-text-secondary">
						<p class="text-text">What this activity has actually returned.</p>
						<p>
							Loot TT counts immediately. Markup enters only after a confirmed sale is
							attributed back to the activities that produced it.
						</p>
					</div>
				</InfoTip>
			</div>
			<div class="grid grid-cols-2 gap-4">
				<StatDisplay
					label="Net"
					value={signedPed(realisedReturns - cycled)}
					valueClass={netTone(realisedReturns - cycled)}
					unit="PED"
				/>
				<StatDisplay label="Rate" value={formatPercent(realisedRate)} />
			</div>
		</section>
	</div>
</div>
