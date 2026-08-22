<script lang="ts">
	import InfoTip from '$lib/components/InfoTip.svelte';
	import RewardMuInfoTip from '$lib/components/RewardMuInfoTip.svelte';
	import type { HuntingRewardStatus } from '$lib/types/analytics';
	import { NO_DATA, formatPed } from '$lib/utils/format';

	let {
		ttPed,
		muPed,
		status,
		scope,
	}: {
		ttPed: number;
		muPed: number | null;
		status: HuntingRewardStatus;
		/** A session states the same reward its activities do, aggregated; the
		 * scope only changes where a mixed provenance is resolved. */
		scope: 'activity' | 'session';
	} = $props();

	const ttValue = $derived.by(() => {
		switch (status) {
			case 'included_in_loot':
				return 'In loot';
			case 'fixed_liquid':
				return `+${formatPed(ttPed)}`;
			case 'item':
				return formatPed(ttPed);
			case 'skill':
				return 'Skill';
			case 'mixed':
				return ttPed > 0 ? formatPed(ttPed) : 'Mixed';
			default:
				return NO_DATA;
		}
	});
	const showsPed = $derived(['fixed_liquid', 'item'].includes(status) || (status === 'mixed' && ttPed > 0));
	const detail = $derived.by(() => {
		switch (status) {
			case 'included_in_loot':
				return 'The completion payout is already present in tracked loot and is not added again.';
			case 'fixed_liquid':
				return 'A separately recorded liquid reward is linked to this completion and added exactly once.';
			case 'item':
				return 'The completion reward was separated from ordinary loot. Its observed TT is counted here; current market value is shown separately as Reward MU.';
			case 'skill':
				return 'The completion paid progression value. It remains outside PED profit.';
			case 'mixed':
				return scope === 'session'
					? 'This session contains more than one reward treatment. Open an activity for its exact provenance.'
					: 'This aggregate contains more than one reward treatment. Open a variant for its exact provenance.';
			case 'unverified':
				return 'Some completions predate immutable reward capture. Their present-day quest settings are not used to rewrite history.';
			default:
				return 'No separate economic reward was recorded for this activity.';
		}
	});
</script>

<div
	class="flex flex-wrap items-center gap-x-5 gap-y-2 pl-1 text-xs text-text-tertiary"
	data-testid="{scope}-reward-context"
>
	<span class="eyebrow">Completion reward</span>
	<span class="flex items-baseline gap-1.5">
		<span>Reward TT</span>
		<strong class="font-medium tabular-nums text-text-secondary">{ttValue}</strong>
		{#if showsPed}<span>PED</span>{/if}
		<InfoTip align="right" width="w-80" label="How this reward is counted">
			<p class="text-xs leading-relaxed text-text-secondary">{detail}</p>
		</InfoTip>
	</span>
	<span class="h-3 w-px bg-border-bright" aria-hidden="true"></span>
	<span class="flex items-baseline gap-1.5" data-testid="{scope}-subordinate-reward-mu">
		<span>Reward MU</span>
		<strong
			class={muPed !== null
				? 'font-medium tabular-nums text-text-secondary'
				: 'font-medium tabular-nums text-text-tertiary'}
		>
			{muPed !== null ? formatPed(muPed) : NO_DATA}
		</strong>
		{#if muPed !== null}<span>PED</span>{/if}
		<RewardMuInfoTip />
	</span>
</div>
