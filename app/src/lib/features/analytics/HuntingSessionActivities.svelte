<script lang="ts">
	import InfoTip from '$lib/components/InfoTip.svelte';
	import StatDisplay from '$lib/components/StatDisplay.svelte';
	import { NO_DATA, formatPed, formatPercent } from '$lib/utils/format';
	import ActivityLootComposition from './ActivityLootComposition.svelte';
	import HuntingActivityPicker from './HuntingActivityPicker.svelte';
	import type { HuntingActivitySection } from './huntingModel.svelte';

	let {
		activities,
		marketAvailable,
	}: {
		activities: HuntingActivitySection[];
		marketAvailable: boolean;
	} = $props();

	let selectedKey = $state<string | null>(null);

	function findActivity(rows: HuntingActivitySection[], key: string): HuntingActivitySection | null {
		for (const row of rows) {
			if (row.key === key) return row;
			const nested = findActivity(row.variants, key);
			if (nested) return nested;
		}
		return null;
	}

	const selected = $derived(
		(selectedKey ? findActivity(activities, selectedKey) : null) ??
			activities.find((activity) => !activity.isUnscoped) ??
			activities[0] ??
			null,
	);

	const signedPed = (value: number) => `${value >= 0 ? '+' : ''}${formatPed(value)}`;
	const kindLabel = (activity: HuntingActivitySection) => {
		switch (activity.kind) {
			case 'quest_family':
				return 'Quest family';
			case 'bundle':
				return 'Joint activity';
			case 'segment':
				return 'Segment';
			case 'ambient':
				return 'Unscoped';
			default:
				return 'Quest';
		}
	};
	const rewardLabel = (activity: HuntingActivitySection) => {
		switch (activity.rewardStatus) {
			case 'included_in_loot':
				return 'Included in loot';
			case 'fixed_liquid':
				return `+${formatPed(activity.confirmedRewardPed)} PED`;
			case 'skill':
				return 'Skill reward';
			case 'mixed':
				return 'Mixed rewards';
			case 'unverified':
				return 'Earlier reward unverified';
			default:
				return null;
		}
	};
	const rewardDetail = (activity: HuntingActivitySection) => {
		switch (activity.rewardStatus) {
			case 'included_in_loot':
				return 'The completion payout is already present in tracked loot and is not added again.';
			case 'fixed_liquid':
				return 'A separately recorded liquid reward is linked to this completion and added exactly once.';
			case 'skill':
				return 'The completion paid progression value. It remains outside PED profit.';
			case 'mixed':
				return 'This aggregate contains more than one reward treatment. Open a variant for its exact provenance.';
			case 'unverified':
				return 'Some completions predate immutable reward capture. Their present-day quest settings are not used to rewrite history.';
			default:
				return 'No separate economic reward was recorded for this activity.';
		}
	};
</script>

{#if selected}
	<div>
		<div class="flex items-start justify-between gap-3">
			<div class="min-w-0">
				<HuntingActivityPicker
					{activities}
					{selected}
					onselect={(key) => (selectedKey = key)}
				/>
				<p class="mt-0.5 text-xs text-text-tertiary">{kindLabel(selected)}</p>
			</div>
			{#if rewardLabel(selected)}
				<div class="flex shrink-0 items-center gap-1.5 rounded-md border border-border/50 bg-surface-hover/30 px-2 py-1 text-xs text-text-secondary">
					<span>{rewardLabel(selected)}</span>
					<InfoTip align="right" width="w-80" label="How this reward is counted">
						<p class="text-xs leading-relaxed text-text-secondary">{rewardDetail(selected)}</p>
					</InfoTip>
				</div>
			{/if}
		</div>

		{#if selected.isUnscoped}
			<p class="mt-5 rounded-lg border border-border/40 bg-surface-hover/20 p-4 text-sm leading-relaxed text-text-secondary">
				This evidence was recorded without a declared quest or segment. It remains in the session total but cannot support a repeatable activity comparison.
			</p>
		{:else}
			<div class="mt-5 grid grid-cols-3 gap-x-5">
				<StatDisplay
					label="TT Net"
					value={signedPed(selected.returns - selected.cycled)}
					unit="PED"
				/>
				<StatDisplay
					label="Reward"
					value={selected.rewardStatus === 'fixed_liquid'
						? `+${formatPed(selected.confirmedRewardPed)}`
						: selected.rewardStatus === 'included_in_loot'
							? 'In loot'
							: selected.rewardStatus === 'skill'
								? 'Skill'
								: NO_DATA}
					unit={selected.rewardStatus === 'fixed_liquid' ? 'PED' : ''}
				/>
				<StatDisplay
					label="Rewarded Net"
					value={selected.rewardStatus === 'unverified'
						? NO_DATA
						: signedPed(selected.rewardedReturns - selected.cycled)}
					valueClass={selected.rewardStatus === 'unverified' ? 'text-text-tertiary' : 'text-text'}
					unit={selected.rewardStatus === 'unverified' ? '' : 'PED'}
				/>
			</div>

			{#if selected.rewardStatus === 'fixed_liquid'}
				<p class="mt-3 text-xs tabular-nums text-text-tertiary">
					{formatPed(selected.returns)} loot + {formatPed(selected.confirmedRewardPed)} reward − {formatPed(selected.cycled)} cycled =
					<span class="font-medium text-text">
						{signedPed(selected.rewardedReturns - selected.cycled)} PED
					</span>
				</p>
			{/if}

			{#if selected.muRewardedReturns !== null}
				<div class="mt-4 flex items-center justify-between rounded-lg border border-border/40 bg-surface-hover/20 px-3 py-2">
					<div>
						<p class="text-xs font-medium text-text">At current market</p>
						<p class="text-[0.6875rem] text-text-tertiary">Projected loot markup plus confirmed liquid reward</p>
					</div>
					<div class="text-right">
						<p class="text-sm font-semibold tabular-nums text-text">
							{signedPed(selected.muRewardedReturns - selected.cycled)} PED
						</p>
						<p class="text-xs tabular-nums text-text-secondary">
							{selected.muRewardedRate !== null ? formatPercent(selected.muRewardedRate) : NO_DATA}
						</p>
					</div>
				</div>
			{/if}

			<ActivityLootComposition
				items={selected.items}
				{marketAvailable}
				emptyLabel="No itemised loot was recorded for this activity."
				disclosure="activity"
			/>
		{/if}
	</div>
{:else}
	<p class="px-2.5 py-4 text-center text-xs text-text-tertiary">
		No quest or segment evidence was recorded for this session.
	</p>
{/if}
