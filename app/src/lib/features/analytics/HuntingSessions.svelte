<script lang="ts">
	import Card from '$lib/components/Card.svelte';
	import InfoTip from '$lib/components/InfoTip.svelte';
	import SegmentedControl from '$lib/components/SegmentedControl.svelte';
	import StatDisplay from '$lib/components/StatDisplay.svelte';
	import type { TableModel } from '$lib/view/tableModel.svelte';
	import { NO_DATA, formatPed, formatPercent } from '$lib/utils/format';
	import ActivityLootComposition from './ActivityLootComposition.svelte';
	import HuntingSessionPicker from './HuntingSessionPicker.svelte';
	import HuntingSessionActivities from './HuntingSessionActivities.svelte';
	import type { HuntingSessionSection } from './huntingModel.svelte';

	let {
		table,
		selected,
		totalCount,
		onselect,
	}: {
		table: TableModel<HuntingSessionSection>;
		selected: HuntingSessionSection | null;
		totalCount: number;
		onselect: (key: string) => void;
	} = $props();

	type DetailView = 'activities' | 'loot';
	let detailView = $state<DetailView>('activities');
	const DETAIL_VIEWS = [
		{ id: 'activities', label: 'Activities' },
		{ id: 'loot', label: 'Loot' },
	];
	const hasDeclaredActivities = $derived(
		selected?.activities.some((activity) => !activity.isUnscoped) ?? false,
	);
	$effect(() => {
		void selected?.key;
		detailView = hasDeclaredActivities ? 'activities' : 'loot';
	});

	const signedPed = (value: number) => `${value >= 0 ? '+' : ''}${formatPed(value)}`;
	const netTone = (value: number) => (value >= 0 ? 'text-positive' : 'text-negative');
</script>

<Card class="hover:z-20">
	{#if selected}
		<div class="min-w-0 p-6">
			{#if selected.isUnassigned}
				<HuntingSessionPicker {table} {selected} {totalCount} {onselect} />
				<div class="mt-5 flex min-h-28 items-center justify-center border-t border-border/50 pt-5">
					<div class="flex items-center gap-1.5 text-sm text-text-secondary">
						<span>Hunting outside a defined session cannot join the routine comparison.</span>
						<InfoTip label="What unassigned hunting means" width="w-80">
							<p class="text-xs font-semibold leading-relaxed text-text">Hunting without a repeatable routine</p>
							<p class="mt-1 text-xs leading-relaxed text-text-secondary">Its cost and loot still count in Overall. Without a session definition there is no deliberate activity to rank, so this diagnostic row carries no economic claim of its own.</p>
						</InfoTip>
					</div>
				</div>
			{:else}
				<div
					class="grid grid-cols-[minmax(10rem,1.35fr)_repeat(3,minmax(0,1fr))] items-start gap-x-6 gap-y-4
						border-b border-border/50 pb-5"
					data-testid="hunting-session-headline"
				>
					<div class="min-w-0">
						<HuntingSessionPicker {table} {selected} {totalCount} {onselect} />
					</div>
					<StatDisplay label="TT Net" value={signedPed(selected.returns - selected.cycled)} unit="PED" />
					<StatDisplay label="MU Net" value={selected.muProjectedReturns !== null ? signedPed(selected.muProjectedReturns - selected.cycled) : NO_DATA} unit={selected.muProjectedReturns !== null ? 'PED' : ''} />
					<StatDisplay label="Realised Net" value={signedPed(selected.realisedReturns - selected.cycled)} valueClass={netTone(selected.realisedReturns - selected.cycled)} unit="PED">
						{#snippet labelSuffix()}
							<InfoTip align="right" width="w-80" label="What Realised Net reports">
								<p class="text-xs font-semibold leading-relaxed text-text">Realised Net: what this session actually achieved</p>
								<p class="mt-1 text-xs leading-relaxed text-text-secondary">Loot TT less cycled PED, plus confirmed markup attributed through the stock this session produced, after auction fees.</p>
							</InfoTip>
						{/snippet}
					</StatDisplay>
					<div class="border-t border-border/35 pt-3" data-testid="session-subordinate-cycled">
						<span class="eyebrow block text-text-tertiary">Cycled</span>
						<div class="mt-1 flex items-baseline gap-1.5">
							<span class="text-base font-semibold tabular-nums text-text-secondary">{formatPed(selected.cycled)}</span>
							<span class="text-[0.625rem] font-medium uppercase tracking-wider text-text-tertiary">PED</span>
						</div>
					</div>
					<div class="border-t border-border/35 pt-3" data-testid="session-subordinate-tt-rate">
						<span class="eyebrow block text-text-tertiary">TT Rate</span>
						<span class="mt-1 block text-base font-semibold tabular-nums {netTone(selected.lootRate - 1)}">
							{formatPercent(selected.lootRate)}
						</span>
					</div>
					<div class="border-t border-border/35 pt-3" data-testid="session-subordinate-mu-rate">
						<span class="eyebrow block text-text-tertiary">MU Rate</span>
						<span class="mt-1 block text-base font-semibold tabular-nums {selected.muRate !== null ? netTone(selected.muRate - 1) : 'text-text-tertiary'}">
							{selected.muRate !== null ? formatPercent(selected.muRate) : NO_DATA}
						</span>
					</div>
					<div class="border-t border-border/35 pt-3" data-testid="session-subordinate-realised-rate">
						<span class="eyebrow block text-text-tertiary">Realised Rate</span>
						<span class="mt-1 block text-base font-semibold tabular-nums {netTone(selected.realisedRate - 1)}">
							{formatPercent(selected.realisedRate)}
						</span>
					</div>
				</div>
				{#if hasDeclaredActivities}
					<div class="mt-4">
						<SegmentedControl
							options={DETAIL_VIEWS}
							active={detailView}
							onchange={(id) => (detailView = id as DetailView)}
						/>
					</div>
				{/if}
				<div class={hasDeclaredActivities ? 'mt-4' : 'mt-5'}>
					{#if detailView === 'activities'}
						<HuntingSessionActivities
							activities={selected.activities}
							marketAvailable={selected.muProjectedReturns !== null}
						/>
					{:else}
						<ActivityLootComposition
							items={selected.items}
							marketAvailable={selected.muProjectedReturns !== null}
							emptyLabel="No loot recorded for this session yet."
						/>
					{/if}
				</div>
			{/if}
		</div>
	{:else}
		<div class="p-8 text-center text-sm text-text-tertiary">No hunting sessions in this period.</div>
	{/if}
</Card>
