<script lang="ts">
	import Card from '$lib/components/Card.svelte';
	import InfoTip from '$lib/components/InfoTip.svelte';
	import StatDisplay from '$lib/components/StatDisplay.svelte';
	import type { SortDir, SortKey } from '$lib/view/tableModel.svelte';
	import ActivityLootComposition from './ActivityLootComposition.svelte';
	import {
		treeCuttingActivityName,
		type TreeCuttingActivitySortKey,
		type TreeCuttingSection,
	} from './treeCuttingModel.svelte';
	import { NO_DATA, formatPed, formatPercent } from '$lib/utils/format';

	let {
		sections,
		selected,
		onselect,
		sortKey,
		sortDir,
		onsort,
	}: {
		sections: TreeCuttingSection[];
		selected: TreeCuttingSection | null;
		onselect: (yieldTier: TreeCuttingSection['yieldTier']) => void;
		sortKey: SortKey<TreeCuttingSection> | undefined;
		sortDir: SortDir;
		onsort: (key: TreeCuttingActivitySortKey) => void;
	} = $props();

	// Unclassified is pinned after the classified activities whatever the sort.
	// This re-partitions downstream of the sort the parent applied, deliberately:
	// it is a diagnostic bucket with its economic columns suppressed, so it has
	// no rank to take part in.
	let displaySections = $derived([
		...sections.filter((section) => section.yieldTier !== 'unknown'),
		...sections.filter((section) => section.yieldTier === 'unknown'),
	]);

	const signedPed = (value: number) => `${value >= 0 ? '+' : ''}${formatPed(value)}`;
	const netTone = (value: number) => (value >= 0 ? 'text-positive' : 'text-negative');
	const rateTone = (value: number) => netTone(value - 1);

	// The list's column widths, declared once because the header and the rows
	// have to shrink identically or they stop lining up.
	//
	// Every column gives ground in proportion to its own width rather than the
	// name column absorbing the whole squeeze: at basis 0 the name collapses
	// first and its text spills over the neighbour, which is what a narrow
	// pane used to look like. `min-w-0` is what lets a column go below the
	// width of its own text instead of stopping there and pushing the row wide.
	const COL_NAME = 'min-w-0 flex-[1_1_6rem]';
	const COL_CYCLED = 'min-w-0 flex-[0_1_3.5rem]';
	const COL_MU = 'min-w-0 flex-[0_1_4rem]';
	const COL_REALISED = 'min-w-0 flex-[0_1_7.5rem]';
	const sortArrow = (key: TreeCuttingActivitySortKey) =>
		sortKey === key ? (sortDir === 'asc' ? '\u2191' : '\u2193') : '';
	const sortDescription = (key: TreeCuttingActivitySortKey, label: string) => {
		if (sortKey !== key) return `Sort by ${label}`;
		return `Sort by ${label}, currently ${sortDir === 'asc' ? 'ascending' : 'descending'}`;
	};

</script>

{#snippet subActivityRow(section: TreeCuttingSection, isSelected: boolean)}
	{@const isUnclassified = section.yieldTier === 'unknown'}
	<li>
		<button
			type="button"
			aria-pressed={isSelected}
			onclick={() => onselect(section.yieldTier)}
			class="w-full flex items-center gap-2 rounded-lg border px-3 py-2 text-left
				transition-[background-color,border-color] duration-[var(--duration-base)] ease-[var(--ease-out)]
				{isSelected
					? 'border-accent/40 bg-accent/[0.08]'
					: 'border-transparent hover:border-border/40 hover:bg-surface-hover/40'}"
		>
			<span
				class="{COL_NAME} truncate text-sm font-medium tracking-tight
					{isUnclassified ? 'text-text-tertiary' : 'text-text'}"
				title={treeCuttingActivityName(section)}
			>
				{treeCuttingActivityName(section)}
			</span>
			{#if isUnclassified}
				<span class="sr-only">Activity metrics not applicable</span>
				<span class={COL_CYCLED} aria-hidden="true"></span>
				<span class={COL_MU} aria-hidden="true"></span>
				<span class={COL_REALISED} aria-hidden="true"></span>
			{:else}
				<span class="{COL_CYCLED} truncate text-right text-xs tabular-nums text-text">
					{formatPed(section.cycled)}
				</span>
				<span class="{COL_MU} truncate text-right text-xs tabular-nums text-text">
					{section.muRate !== null ? formatPercent(section.muRate) : NO_DATA}
				</span>
				<span
					class="{COL_REALISED} truncate text-right text-xs tabular-nums font-medium {rateTone(
						section.realisedRate,
					)}"
				>
					{formatPercent(section.realisedRate)}
				</span>
			{/if}
		</button>
	</li>
{/snippet}

<Card class="hover:z-20">
	<!-- The list pane's width is shared with the market panel the toggle swaps
		to: the two are the same frame with different contents, so the hairline
		has to fall in the same place in both.

		A proportion, not a floor. A minimum width on this track refuses to give
		anything back as the card narrows, so the whole of an expanding
		sidebar's cost lands on the detail pane and its rightmost column leaves
		the screen. Both sides narrow together instead, and the column headers
		below are free to wrap once they are genuinely short of room. -->
	<div class="grid sm:grid-cols-[46%_minmax(0,1fr)]">
		<div class="min-w-0 border-b border-border/40 sm:border-b-0 sm:border-r">
			<div class="px-2 pt-4">
				<div
					class="flex items-center gap-2 rounded-lg border border-transparent px-3 pb-2 text-text-tertiary"
				>
					<button
						type="button"
						class="eyebrow {COL_NAME} flex cursor-pointer items-center gap-1 text-left transition-colors duration-[var(--duration-fast)] hover:text-text"
						aria-label={sortDescription('yieldTier', 'Activity')}
						onclick={() => onsort('yieldTier')}
					>
						Activity
						{#if sortKey === 'yieldTier'}<span class="text-accent">{sortArrow('yieldTier')}</span>{/if}
					</button>
					<button
						type="button"
						class="eyebrow {COL_CYCLED} flex cursor-pointer items-center justify-end gap-1 text-right transition-colors duration-[var(--duration-fast)] hover:text-text"
						aria-label={sortDescription('cycled', 'Cycled')}
						onclick={() => onsort('cycled')}
					>
						Cycled
						{#if sortKey === 'cycled'}<span class="text-accent">{sortArrow('cycled')}</span>{/if}
					</button>
					<button
						type="button"
						class="eyebrow {COL_MU} flex cursor-pointer items-center justify-end gap-1 text-right transition-colors duration-[var(--duration-fast)] hover:text-text"
						aria-label={sortDescription('muRate', 'MU Rate')}
						onclick={() => onsort('muRate')}
					>
						MU Rate
						{#if sortKey === 'muRate'}<span class="text-accent">{sortArrow('muRate')}</span>{/if}
					</button>
					<button
						type="button"
						class="eyebrow {COL_REALISED} flex cursor-pointer items-center justify-end gap-1 text-right transition-colors duration-[var(--duration-fast)] hover:text-text"
						aria-label={sortDescription('realisedRate', 'Realised Rate')}
						onclick={() => onsort('realisedRate')}
					>
						Realised Rate
						{#if sortKey === 'realisedRate'}<span class="text-accent">{sortArrow('realisedRate')}</span>{/if}
					</button>
				</div>
			</div>
			<ul class="flex max-h-[32rem] flex-col gap-1 overflow-y-auto px-2 pb-3">
				{#each displaySections as section (section.yieldTier)}
					{@render subActivityRow(section, section.yieldTier === selected?.yieldTier)}
				{/each}
			</ul>
		</div>

		{#if selected}
			<!-- One scroll region bounded to the list pane's own height, so the
				two sides of the hairline stay the same height; on this tab's
				short content it simply never engages. -->
			<div class="min-w-0 max-h-[32rem] overflow-y-auto p-5">
				{#if selected.yieldTier === 'unknown'}
					<div class="flex min-h-28 items-center justify-center">
						<div class="flex items-center gap-1.5 text-sm text-text-secondary">
							<span>
								{selected.swings}
								{selected.swings === 1 ? 'swing is' : 'swings are'} unclassified and cannot be
								assigned to a board activity.
							</span>
							<InfoTip label="Why swings can be unclassified" width="w-80">
								<p class="text-xs font-semibold leading-relaxed text-text">
									Why swings can be unclassified
								</p>
								<p class="mt-1 text-xs leading-relaxed text-text-secondary">
									A swing is unclassified when no board output identifies its activity. This can
									happen on a failed or shavings-only swing without nearby board evidence from the
									same tool and hotkey run, when neighbouring evidence conflicts, or when a board
									name is not recognised.
								</p>
								<p class="mt-2 text-xs leading-relaxed text-text-tertiary">
									Its recorded cost and loot still count in Overall. They cannot be assigned to
									Short Boards, Boards, or Long Boards, so a large unclassified count makes the
									activity comparison less complete.
								</p>
							</InfoTip>
						</div>
					</div>
				{:else}
				<div class="grid grid-cols-3 gap-x-5">
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
					/>
					<StatDisplay
						label="Realised Net"
						value={signedPed(selected.realisedReturns - selected.cycled)}
						valueClass={netTone(selected.realisedReturns - selected.cycled)}
						unit="PED"
					>
						{#snippet labelSuffix()}
							<InfoTip align="right" width="w-80" label="What Realised Net reports">
								<p class="text-xs font-semibold leading-relaxed text-text">
									Realised Net: what this activity actually achieved
								</p>
								<p class="mt-1 text-xs leading-relaxed text-text-secondary">
									Loot TT less cycled PED, plus the markup confirmed sales of this activity's
									output have realised, after auction fees.
								</p>
								<p class="mt-2 text-xs leading-relaxed text-text-tertiary">
									It reads the same as TT Net until stock this activity produced is sold and the
									sale confirmed, because until then no markup has been realised. A sale recorded
									directly in the Ledger carries no link to an activity and does not reach here.
								</p>
							</InfoTip>
						{/snippet}
					</StatDisplay>
				</div>

				<ActivityLootComposition
					items={selected.items}
					marketAvailable={selected.muProjectedReturns !== null}
					emptyLabel="No loot recorded for this board activity yet."
				/>
				{/if}
			</div>
		{/if}
	</div>
</Card>
