<script lang="ts">
	import type { Snippet } from 'svelte';
	import { NO_DATA, formatPed, formatPercent } from '$lib/utils/format';

	let {
		cycled,
		ttNet,
		ttRate,
		muNet,
		muRate,
		realisedNet,
		realisedRate,
		muTip,
		realisedTip,
		testid = 'activity-economic-headline',
	}: {
		cycled: number;
		ttNet: number;
		ttRate: number;
		muNet: number | null;
		muRate: number | null;
		realisedNet?: number | null;
		realisedRate?: number | null;
		muTip?: Snippet;
		realisedTip?: Snippet;
		testid?: string;
	} = $props();

	const signedPed = (value: number) => `${value >= 0 ? '+' : ''}${formatPed(value)}`;
	const realisedTone = (value: number) => (value >= 0 ? 'text-positive' : 'text-negative');
</script>

<section class="economic-horizon relative" data-testid={testid}>
	<div class="pointer-events-none absolute -left-8 top-8 h-20 w-56 rounded-full bg-accent-faint blur-3xl" aria-hidden="true"></div>
	<div class="relative grid grid-cols-1 gap-x-7 gap-y-6 sm:grid-cols-3 lg:grid-cols-[minmax(11rem,1.2fr)_repeat(3,minmax(0,1fr))]">
		<div class="min-w-0 sm:col-span-3 lg:col-span-1">
			<div class="flex items-baseline gap-2" data-testid="economic-subordinate-cycled">
				<span class="text-lg font-medium tabular-nums text-text-secondary">{formatPed(cycled)}</span>
				<span class="eyebrow">PED cycled</span>
			</div>
		</div>

		<div class="outcome outcome-tt">
			<div class="outcome-label"><span class="outcome-dot"></span><span>TT Net</span></div>
			<div class="outcome-value text-text">{signedPed(ttNet)} <span>PED</span></div>
			<div class="outcome-rate" data-testid="economic-subordinate-tt-rate">
				<span>TT Rate</span>
				<strong class="text-text">{formatPercent(ttRate)}</strong>
			</div>
		</div>

		<div class="outcome outcome-mu">
			<div class="outcome-label">
				<span class="outcome-dot"></span><span>MU Net</span>
				{#if muTip}{@render muTip()}{/if}
			</div>
			{#if muNet !== null}
				<div class="outcome-value text-text">{signedPed(muNet)} <span>PED</span></div>
			{:else}
				<div class="outcome-value text-text-tertiary">{NO_DATA}</div>
			{/if}
			<div class="outcome-rate" data-testid="economic-subordinate-mu-rate">
				<span>MU Rate</span>
				<strong class={muRate !== null ? 'text-text' : 'text-text-tertiary'}>
					{muRate !== null ? formatPercent(muRate) : NO_DATA}
				</strong>
			</div>
		</div>

		<div class="outcome outcome-realised">
			<div class="outcome-label">
				<span class="outcome-dot"></span><span>Realised Net</span>
				{#if realisedTip}{@render realisedTip()}{/if}
			</div>
			{#if realisedNet !== null && realisedNet !== undefined}
				<div class="outcome-value {realisedTone(realisedNet)}">{signedPed(realisedNet)} <span>PED</span></div>
			{:else}
				<div class="outcome-value text-text-tertiary">{NO_DATA}</div>
			{/if}
			<div class="outcome-rate" data-testid="economic-subordinate-realised-rate">
				<span>Realised Rate</span>
				<strong class={realisedRate !== null && realisedRate !== undefined ? realisedTone(realisedRate - 1) : 'text-text-tertiary'}>
					{realisedRate !== null && realisedRate !== undefined ? formatPercent(realisedRate) : NO_DATA}
				</strong>
			</div>
		</div>
	</div>
</section>

<style>
	.outcome {
		--outcome-colour: var(--color-border-bright);
		position: relative;
		min-width: 0;
		padding-left: 1rem;
	}

	.outcome::before {
		content: '';
		position: absolute;
		inset: 0 auto 0 0;
		width: 1px;
		background: linear-gradient(to bottom, var(--outcome-colour), color-mix(in oklab, var(--outcome-colour) 8%, transparent));
		opacity: 0.7;
	}

	.outcome-tt { --outcome-colour: var(--color-accent); }
	.outcome-mu { --outcome-colour: #a78bfa; }
	.outcome-realised { --outcome-colour: var(--color-text-tertiary); }

	.outcome-label {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		font-size: 10.5px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.16em;
		color: var(--color-text-tertiary);
	}

	.outcome-dot {
		width: 0.35rem;
		height: 0.35rem;
		border-radius: 9999px;
		background: var(--outcome-colour);
		box-shadow: 0 0 9px color-mix(in oklab, var(--outcome-colour) 55%, transparent);
	}

	.outcome-value {
		margin-top: 0.65rem;
		font-size: clamp(1.55rem, 2.4vw, 2.15rem);
		font-weight: 650;
		font-variant-numeric: tabular-nums;
		line-height: 1;
		letter-spacing: -0.025em;
	}

	.outcome-value span {
		font-size: 0.65rem;
		font-weight: 600;
		letter-spacing: 0.1em;
		color: var(--color-text-tertiary);
	}

	.outcome-rate {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 0.75rem;
		margin-top: 0.8rem;
		padding-top: 0.55rem;
		border-top: 1px solid color-mix(in oklab, var(--outcome-colour) 22%, var(--color-border));
		font-size: 0.7rem;
		color: var(--color-text-tertiary);
	}

	.outcome-rate strong {
		font-size: 0.82rem;
		font-weight: 600;
		font-variant-numeric: tabular-nums;
	}
</style>
