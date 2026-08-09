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
	const netTone = (value: number) => (value >= 0 ? 'text-positive' : 'text-negative');
	const muRate = (activity: HuntingActivitySection) =>
		activity.muProjectedReturns !== null && activity.cycled > 0
			? activity.muProjectedReturns / activity.cycled
			: null;
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
	const rewardValue = (activity: HuntingActivitySection) => {
		switch (activity.rewardStatus) {
			case 'included_in_loot':
				return 'In loot';
			case 'fixed_liquid':
				return `+${formatPed(activity.confirmedRewardPed)}`;
			case 'skill':
				return 'Skill';
			case 'mixed':
				return 'Mixed';
			case 'unverified':
				return NO_DATA;
			default:
				return NO_DATA;
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

{#snippet rewardTip()}
	{#if selected}
		<InfoTip align="right" width="w-80" label="How this reward is counted">
			<p class="text-xs leading-relaxed text-text-secondary">{rewardDetail(selected)}</p>
		</InfoTip>
	{/if}
{/snippet}

{#snippet estimateTip()}
	<InfoTip label="What MU figures are">
		<p class="text-xs font-semibold leading-relaxed text-text">Estimated, not realised</p>
		<p class="mt-1 text-xs leading-relaxed text-text-secondary">
			What current market markup would add if this activity's loot sold at it. Confirmed rewards
			remain outside this estimate.
		</p>
	</InfoTip>
{/snippet}

{#snippet realisedTip()}
	<InfoTip label="What realised figures are">
		<p class="text-xs font-semibold leading-relaxed text-text">Loot TT plus confirmed liquid reward</p>
		<p class="mt-1 text-xs leading-relaxed text-text-secondary">
			At activity grain, a separately confirmed liquid quest reward is added exactly once. Confirmed
			sale markup remains attributed at session grain.
		</p>
	</InfoTip>
{/snippet}

{#snippet rewardMuTip()}
	<InfoTip label="What Reward MU is">
		<p class="text-xs font-semibold leading-relaxed text-text">Projected, not realised</p>
		<p class="mt-1 text-xs leading-relaxed text-text-secondary">
			The actual reward item recorded at completion, valued from its current market data. An item
			without usable market data stays at TT. Realised figures continue to use confirmed value only.
		</p>
	</InfoTip>
{/snippet}

{#if selected}
	<div>
		<div class="min-w-0">
			<div class="w-full min-w-0">
				<HuntingActivityPicker
					{activities}
					{selected}
					onselect={(key) => (selectedKey = key)}
				/>
				<p class="mt-0.5 text-xs text-text-tertiary">{kindLabel(selected)}</p>
			</div>
		</div>

		{#if selected.isUnscoped}
			<p class="mt-5 rounded-lg border border-border/40 bg-surface-hover/20 p-4 text-sm leading-relaxed text-text-secondary">
				This evidence was recorded without a declared quest or segment. It remains in the session total but cannot support a repeatable activity comparison.
			</p>
		{:else}
			<div
				class="mt-5 grid grid-cols-4 items-start gap-x-6 gap-y-4"
				data-testid="activity-economic-grid"
			>
				<StatDisplay
					label="Reward"
					value={rewardValue(selected)}
					unit={selected.rewardStatus === 'fixed_liquid' ? 'PED' : ''}
					labelSuffix={rewardTip}
				/>
				<StatDisplay
					label="TT Net"
					value={signedPed(selected.returns - selected.cycled)}
					unit="PED"
				/>
				<StatDisplay
					label="MU Net"
					value={selected.muProjectedReturns !== null
						? signedPed(selected.muProjectedReturns - selected.cycled)
						: NO_DATA}
					unit={selected.muProjectedReturns !== null ? 'PED' : ''}
					labelSuffix={estimateTip}
				/>
				<StatDisplay
					label="Realised Net"
					value={selected.rewardStatus === 'unverified'
						? NO_DATA
						: signedPed(selected.rewardedReturns - selected.cycled)}
					valueClass={selected.rewardStatus === 'unverified'
						? 'text-text-tertiary'
						: netTone(selected.rewardedReturns - selected.cycled)}
					unit={selected.rewardStatus === 'unverified' ? '' : 'PED'}
					labelSuffix={realisedTip}
				/>

				<div class="border-t border-border/35 pt-3" data-testid="activity-subordinate-reward-mu">
					<StatDisplay
						label="Reward MU"
						value={selected.rewardMuPed !== null ? formatPed(selected.rewardMuPed) : NO_DATA}
						unit={selected.rewardMuPed !== null ? 'PED' : ''}
						valueClass={selected.rewardMuPed !== null ? 'text-text' : 'text-text-tertiary'}
						labelSuffix={rewardMuTip}
						emphasis="secondary"
					/>
				</div>
				<div class="border-t border-border/35 pt-3" data-testid="activity-subordinate-tt-rate">
					<StatDisplay label="TT Rate" value={formatPercent(selected.lootRate)} valueClass="text-text" emphasis="secondary" />
				</div>
				<div class="border-t border-border/35 pt-3" data-testid="activity-subordinate-mu-rate">
					<StatDisplay
						label="MU Rate"
						value={muRate(selected) !== null ? formatPercent(muRate(selected) ?? 0) : NO_DATA}
						valueClass={muRate(selected) !== null ? 'text-text' : 'text-text-tertiary'}
						emphasis="secondary"
					/>
				</div>
				<div class="border-t border-border/35 pt-3" data-testid="activity-subordinate-realised-rate">
					<StatDisplay
						label="Realised Rate"
						value={selected.rewardStatus === 'unverified'
							? NO_DATA
							: formatPercent(selected.rewardedRate)}
						valueClass={selected.rewardStatus === 'unverified'
							? 'text-text-tertiary'
							: netTone(selected.rewardedRate - 1)}
						emphasis="secondary"
					/>
				</div>
			</div>

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
