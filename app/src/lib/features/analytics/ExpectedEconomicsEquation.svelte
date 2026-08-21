<script lang="ts">
	import type { Snippet } from 'svelte';
	import { NO_DATA, formatPercent } from '$lib/utils/format';

	let {
		effectiveEfficiency,
		lootMarkupFactor,
		expectedTtRate,
		expectedMarketRate,
		efficiencyTip,
		estimateTip,
		expectedTip,
		testid = 'hunting-expected-economics',
	}: {
		effectiveEfficiency: string;
		lootMarkupFactor: number | null;
		expectedTtRate: number | null;
		expectedMarketRate: number | null;
		efficiencyTip?: Snippet;
		estimateTip?: Snippet;
		expectedTip?: Snippet;
		testid?: string;
	} = $props();
</script>

<section class="long-run relative mt-7 overflow-hidden border-y border-border/45 py-5" data-testid={testid}>
	<div class="pointer-events-none absolute inset-y-0 right-0 w-2/3 bg-[radial-gradient(ellipse_at_right,color-mix(in_oklab,var(--color-accent)_10%,transparent),transparent_68%)]" aria-hidden="true"></div>
	<div class="relative flex flex-wrap items-center justify-between gap-x-8 gap-y-5">
		<div class="min-w-36">
			<p class="eyebrow-strong text-accent">Long-run outlook</p>
			<p class="mt-1 text-xs leading-relaxed text-text-tertiary">Offensive costs only</p>
		</div>

		<div class="equation flex min-w-0 flex-1 flex-wrap items-end justify-end gap-x-5 gap-y-4">
			<div class="equation-term">
				<div class="equation-label">Expected Return {#if expectedTip}{@render expectedTip()}{/if}</div>
				<div class={expectedTtRate !== null ? 'equation-value text-text' : 'equation-value text-text-tertiary'}>
					{expectedTtRate !== null ? formatPercent(expectedTtRate) : NO_DATA}
				</div>
				<div class="equation-context">
					<span>Effective Efficiency</span>
					<strong>{effectiveEfficiency}</strong>
					{#if efficiencyTip}{@render efficiencyTip()}{/if}
				</div>
			</div>

			<span class="equation-operator" aria-hidden="true">×</span>

			<div class="equation-term">
				<div class="equation-label">Loot MU {#if estimateTip}{@render estimateTip()}{/if}</div>
				<div class={lootMarkupFactor !== null ? 'equation-value text-text' : 'equation-value text-text-tertiary'}>
					{lootMarkupFactor !== null ? formatPercent(lootMarkupFactor) : NO_DATA}
				</div>
				<div class="equation-context">100%-anchored loot mix</div>
			</div>

			<span class="equation-operator" aria-hidden="true">=</span>

			<div class="equation-result">
				<div class="equation-label">Expected + MU {#if expectedTip}{@render expectedTip()}{/if}</div>
				<div class={expectedMarketRate !== null ? 'result-value text-text' : 'result-value text-text-tertiary'}>
					{expectedMarketRate !== null ? formatPercent(expectedMarketRate) : NO_DATA}
				</div>
				<div class="equation-context">Long-run economic rate</div>
			</div>
		</div>
	</div>
</section>

<style>
	.equation-term,
	.equation-result {
		min-width: 8.5rem;
	}

	.equation-label {
		display: flex;
		align-items: center;
		gap: 0.35rem;
		font-size: 10.5px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.14em;
		color: var(--color-text-tertiary);
	}

	.equation-value,
	.result-value {
		margin-top: 0.45rem;
		font-size: 1.65rem;
		font-weight: 620;
		font-variant-numeric: tabular-nums;
		line-height: 1;
		letter-spacing: -0.02em;
	}

	.result-value {
		font-size: clamp(2rem, 3.4vw, 2.8rem);
	}

	.equation-result {
		padding-left: 0.2rem;
	}

	.equation-context {
		display: flex;
		align-items: center;
		gap: 0.35rem;
		margin-top: 0.55rem;
		font-size: 0.66rem;
		color: var(--color-text-tertiary);
	}

	.equation-context strong {
		font-weight: 600;
		font-variant-numeric: tabular-nums;
		color: var(--color-text-secondary);
	}

	.equation-operator {
		align-self: center;
		padding-bottom: 0.9rem;
		font-size: 1.1rem;
		font-weight: 300;
		color: color-mix(in oklab, var(--color-accent) 55%, var(--color-text-tertiary));
	}

	@media (max-width: 640px) {
		.equation {
			justify-content: flex-start;
		}

		.equation-term,
		.equation-result {
			min-width: 7.25rem;
		}
	}
</style>
