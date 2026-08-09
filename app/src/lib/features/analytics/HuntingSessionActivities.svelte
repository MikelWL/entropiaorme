<script lang="ts">
	import InfoTip from '$lib/components/InfoTip.svelte';
	import SearchInput from '$lib/components/SearchInput.svelte';
	import StatDisplay from '$lib/components/StatDisplay.svelte';
	import { NO_DATA, formatPed, formatPercent } from '$lib/utils/format';
	import ActivityLootComposition from './ActivityLootComposition.svelte';
	import type { HuntingActivitySection } from './huntingModel.svelte';

	let {
		activities,
		marketAvailable,
	}: {
		activities: HuntingActivitySection[];
		marketAvailable: boolean;
	} = $props();

	const SEARCH_THRESHOLD = 8;
	let query = $state('');
	let selectedKey = $state<string | null>(null);

	function findActivity(rows: HuntingActivitySection[], key: string): HuntingActivitySection | null {
		for (const row of rows) {
			if (row.key === key) return row;
			const nested = findActivity(row.variants, key);
			if (nested) return nested;
		}
		return null;
	}

	const selected = $derived(selectedKey ? findActivity(activities, selectedKey) : null);
	const searchable = $derived(activities.length > SEARCH_THRESHOLD || query !== '');
	const filtered = $derived(
		query.trim() === ''
			? activities
			: activities.filter((activity) =>
					activity.label.toLowerCase().includes(query.trim().toLowerCase()),
				),
	);
	const displayActivities = $derived([
		...filtered.filter((activity) => !activity.isUnscoped),
		...filtered.filter((activity) => activity.isUnscoped),
	]);

	const signedPed = (value: number) => `${value >= 0 ? '+' : ''}${formatPed(value)}`;
	const netTone = (value: number) => (value >= 0 ? 'text-positive' : 'text-negative');
	const rateTone = (value: number) => netTone(value - 1);
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
				<button
					type="button"
					class="mb-2 text-xs font-medium text-text-tertiary hover:text-accent"
					onclick={() => (selectedKey = null)}
				>
					← Activities
				</button>
				<h3 class="truncate text-base font-semibold tracking-tight text-text" title={selected.label}>
					{selected.label}
				</h3>
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
					valueClass={netTone(selected.returns - selected.cycled)}
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
					valueClass={selected.rewardStatus === 'unverified'
						? 'text-text-tertiary'
						: netTone(selected.rewardedReturns - selected.cycled)}
					unit={selected.rewardStatus === 'unverified' ? '' : 'PED'}
				/>
			</div>

			{#if selected.rewardStatus === 'fixed_liquid'}
				<p class="mt-3 text-xs tabular-nums text-text-tertiary">
					{formatPed(selected.returns)} loot + {formatPed(selected.confirmedRewardPed)} reward − {formatPed(selected.cycled)} cycled =
					<span class="font-medium {netTone(selected.rewardedReturns - selected.cycled)}">
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
						<p class="text-sm font-semibold tabular-nums {netTone(selected.muRewardedReturns - selected.cycled)}">
							{signedPed(selected.muRewardedReturns - selected.cycled)} PED
						</p>
						<p class="text-xs tabular-nums text-text-secondary">
							{selected.muRewardedRate !== null ? formatPercent(selected.muRewardedRate) : NO_DATA}
						</p>
					</div>
				</div>
			{/if}

			{#if selected.variants.length > 0}
				<div class="mt-5 border-t border-border/50 pt-4">
					<p class="eyebrow px-2.5 pb-2">Variants</p>
					<ul class="flex flex-col gap-1">
						{#each selected.variants as variant (variant.key)}
							<li>
								<button
									type="button"
									class="flex w-full items-center gap-3 rounded-md border border-transparent px-2.5 py-2 text-left hover:border-border/40 hover:bg-surface-hover/30"
									onclick={() => (selectedKey = variant.key)}
								>
									<span class="min-w-0 flex-1 truncate text-sm font-medium text-text">{variant.label}</span>
									<span class="text-xs tabular-nums text-text-secondary">{formatPed(variant.cycled)} PED</span>
									<span class="w-16 text-right text-xs font-medium tabular-nums {rateTone(variant.rewardedRate)}">{formatPercent(variant.rewardedRate)}</span>
								</button>
							</li>
						{/each}
					</ul>
				</div>
			{/if}

			<ActivityLootComposition
				items={selected.items}
				{marketAvailable}
				emptyLabel="No itemised loot was recorded for this activity."
			/>
		{/if}
	</div>
{:else if activities.length > 0}
	<div>
		{#if searchable}
			<div class="px-2.5 pb-3">
				<SearchInput bind:value={query} placeholder="Find an activity" aria-label="Find an activity" />
			</div>
		{/if}
		<div class="flex items-center gap-3 px-2.5 pb-2 text-text-tertiary">
			<span class="eyebrow min-w-0 flex-1">Activity</span>
			<span class="eyebrow w-20 shrink-0 text-right">Cycled</span>
			<span class="eyebrow w-24 shrink-0 text-right">TT → Rewarded</span>
		</div>
		<ul class="flex flex-col gap-1">
			{#each displayActivities as activity (activity.key)}
				<li>
					<button
						type="button"
						class="flex w-full items-center gap-3 rounded-lg border border-transparent px-2.5 py-2 text-left transition-[background-color,border-color] duration-[var(--duration-base)] ease-[var(--ease-out)] hover:border-border/40 hover:bg-surface-hover/30"
						onclick={() => (selectedKey = activity.key)}
					>
						<span class="min-w-0 flex-1">
							<span class="block truncate text-sm font-medium tracking-tight {activity.isUnscoped ? 'text-text-tertiary' : 'text-text'}" title={activity.label}>{activity.label}</span>
							{#if !activity.isUnscoped && rewardLabel(activity)}
								<span class="block truncate text-[0.6875rem] text-text-tertiary">{rewardLabel(activity)}</span>
							{/if}
						</span>
						{#if activity.isUnscoped}
							<span class="w-20" aria-hidden="true"></span>
							<span class="w-24 text-right text-xs text-text-tertiary">Not ranked</span>
						{:else}
							<span class="w-20 shrink-0 text-right text-xs tabular-nums text-text">{formatPed(activity.cycled)}</span>
							<span class="w-24 shrink-0 text-right text-xs font-medium tabular-nums {rateTone(activity.rewardedRate)}">
								{#if activity.rewardStatus === 'fixed_liquid' && activity.confirmedRewardPed > 0}
									<span class="text-text-tertiary">{formatPercent(activity.lootRate)}</span>
									<span class="px-0.5 text-text-tertiary">→</span>
								{/if}
								{activity.rewardStatus === 'unverified' ? NO_DATA : formatPercent(activity.rewardedRate)}
							</span>
						{/if}
					</button>
				</li>
			{/each}
			{#if displayActivities.length === 0}
				<li class="px-3 py-4 text-center text-xs text-text-tertiary">No activity matches that search.</li>
			{/if}
		</ul>
	</div>
{:else}
	<p class="px-2.5 py-4 text-center text-xs text-text-tertiary">
		No quest or segment evidence was recorded for this session.
	</p>
{/if}
