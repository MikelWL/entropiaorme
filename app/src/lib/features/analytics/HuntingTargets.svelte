<script lang="ts">
	/**
	 * The observed Targets axis: mob species compared on the same frame,
	 * columns, and market treatment as the Tree Cutting sub-activities, so
	 * a player who knows one tab is already at home in the other. Species is
	 * the decision unit a player deliberately repeats.
	 *
	 * Species accumulate over a hunting career, so the list carries a quiet
	 * search once it is long enough to need one, inside the same bounded
	 * scroll the sibling panes use.
	 */
	import Card from '$lib/components/Card.svelte';
	import InfoTip from '$lib/components/InfoTip.svelte';
	import SearchInput from '$lib/components/SearchInput.svelte';
	import StatDisplay from '$lib/components/StatDisplay.svelte';
	import type { TableModel } from '$lib/view/tableModel.svelte';
	import { NO_DATA, formatPed, formatPercent } from '$lib/utils/format';
	import ActivityLootComposition from './ActivityLootComposition.svelte';
	import type { HuntingTargetSection, HuntingTargetSortKey } from './huntingModel.svelte';

	let {
		table,
		selected,
		onselect,
	}: {
		table: TableModel<HuntingTargetSection>;
		selected: HuntingTargetSection | null;
		onselect: (key: string) => void;
	} = $props();

	// A species list grows for as long as the player hunts; the search
	// appears once scanning stops being quicker than typing, and stays
	// visible while a query is live so a filter can always be cleared.
	const SEARCH_THRESHOLD = 8;
	const searchable = $derived(table.filtered.length > SEARCH_THRESHOLD || table.search !== '');

	// Unclassified is pinned after the identified species whatever the sort:
	// a diagnostic bucket with its economic columns suppressed has no rank
	// to take part in.
	let displaySections = $derived([
		...table.filtered.filter((section) => !section.isUnclassified),
		...table.filtered.filter((section) => section.isUnclassified),
	]);

	let detailPane = $state<HTMLElement | null>(null);
	$effect(() => {
		void selected?.key;
		if (detailPane) detailPane.scrollTop = 0;
	});

	const signedPed = (value: number) => `${value >= 0 ? '+' : ''}${formatPed(value)}`;
	const netTone = (value: number) => (value >= 0 ? 'text-positive' : 'text-negative');
	const rateTone = (value: number) => netTone(value - 1);

	// The list's column widths, declared once because the header and the rows
	// have to shrink identically or they stop lining up. Kept in step with the
	// Tree Cutting sub-activity list so the two tabs read as one family.
	const COL_NAME = 'min-w-0 flex-[1_1_6rem]';
	const COL_CYCLED = 'min-w-0 flex-[0_1_3.5rem]';
	const COL_MU = 'min-w-0 flex-[0_1_4rem]';
	const COL_REALISED = 'min-w-0 flex-[0_1_7.5rem]';
	const sortArrow = (key: HuntingTargetSortKey) =>
		table.sortKey === key ? (table.sortDir === 'asc' ? '↑' : '↓') : '';
	const sortDescription = (key: HuntingTargetSortKey, label: string) => {
		if (table.sortKey !== key) return `Sort by ${label}`;
		return `Sort by ${label}, currently ${table.sortDir === 'asc' ? 'ascending' : 'descending'}`;
	};
</script>

{#snippet targetRow(section: HuntingTargetSection, isSelected: boolean)}
	<li>
		<button
			type="button"
			aria-pressed={isSelected}
			onclick={() => onselect(section.key)}
			class="w-full flex items-center gap-2 rounded-lg border px-3 py-2 text-left
				transition-[background-color,border-color] duration-[var(--duration-base)] ease-[var(--ease-out)]
				{isSelected
					? 'border-accent/40 bg-accent/[0.08]'
					: 'border-transparent hover:border-border/40 hover:bg-surface-hover/40'}"
		>
			<span
				class="{COL_NAME} truncate text-sm font-medium tracking-tight
					{section.isUnclassified ? 'text-text-tertiary' : 'text-text'}"
				title={section.label}
			>
				{section.label}
			</span>
			{#if section.isUnclassified}
				<span class="sr-only">Target metrics not applicable</span>
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
	<!-- The list pane's width is shared with the sibling panels the toggle
		swaps to: the same frame with different contents, so the hairline has
		to fall in the same place in all of them. -->
	<div class="grid sm:grid-cols-[46%_minmax(0,1fr)]">
		<div class="min-w-0 border-b border-border/40 sm:border-b-0 sm:border-r">
			<div class="px-2 pt-4">
				{#if searchable}
					<div class="px-3 pb-2">
						<SearchInput
							bind:value={table.search}
							placeholder="Find a species"
							aria-label="Find a species"
						/>
					</div>
				{/if}
				<div
					class="flex items-center gap-2 rounded-lg border border-transparent px-3 pb-2 text-text-tertiary"
				>
					<button
						type="button"
						class="eyebrow {COL_NAME} flex cursor-pointer items-center gap-1 text-left transition-colors duration-[var(--duration-fast)] hover:text-text"
						aria-label={sortDescription('label', 'Species')}
						onclick={() => table.setSort('label')}
					>
						Species
						{#if table.sortKey === 'label'}<span class="text-accent">{sortArrow('label')}</span>{/if}
					</button>
					<button
						type="button"
						class="eyebrow {COL_CYCLED} flex cursor-pointer items-center justify-end gap-1 text-right transition-colors duration-[var(--duration-fast)] hover:text-text"
						aria-label={sortDescription('cycled', 'Cycled')}
						onclick={() => table.setSort('cycled')}
					>
						Cycled
						{#if table.sortKey === 'cycled'}<span class="text-accent">{sortArrow('cycled')}</span>{/if}
					</button>
					<button
						type="button"
						class="eyebrow {COL_MU} flex cursor-pointer items-center justify-end gap-1 text-right transition-colors duration-[var(--duration-fast)] hover:text-text"
						aria-label={sortDescription('muRate', 'MU Rate')}
						onclick={() => table.setSort('muRate')}
					>
						MU Rate
						{#if table.sortKey === 'muRate'}<span class="text-accent">{sortArrow('muRate')}</span>{/if}
					</button>
					<button
						type="button"
						class="eyebrow {COL_REALISED} flex cursor-pointer items-center justify-end gap-1 text-right transition-colors duration-[var(--duration-fast)] hover:text-text"
						aria-label={sortDescription('realisedRate', 'Realised Rate')}
						onclick={() => table.setSort('realisedRate')}
					>
						Realised Rate
						{#if table.sortKey === 'realisedRate'}<span class="text-accent"
								>{sortArrow('realisedRate')}</span
							>{/if}
					</button>
				</div>
			</div>
			<ul class="flex max-h-[32rem] flex-col gap-1 overflow-y-auto px-2 pb-3">
				{#each displaySections as section (section.key)}
					{@render targetRow(section, section.key === selected?.key)}
				{/each}
				{#if displaySections.length === 0}
					<li class="px-3 py-4 text-center text-xs text-text-tertiary">
						No species matches that search.
					</li>
				{/if}
			</ul>
		</div>

		{#if selected}
			<!-- One scroll region bounded to the list pane's own height, so the
				two sides of the hairline stay the same height and the pane never
				stacks nested scrollers. A new selection starts at the top. -->
			<div bind:this={detailPane} class="min-w-0 max-h-[32rem] overflow-y-auto p-5">
				{#if selected.isUnclassified}
					<div class="flex min-h-28 items-center justify-center">
						<div class="flex items-center gap-1.5 text-sm text-text-secondary">
							<span>Some hunting could not be assigned to a species.</span>
							<InfoTip label="Why hunting can be unclassified" width="w-80">
								<p class="text-xs font-semibold leading-relaxed text-text">
									Why hunting can be unclassified
								</p>
								<p class="mt-1 text-xs leading-relaxed text-text-secondary">
									Hunting is unclassified when no species was recorded for it: sessions tracked
									before species stamping existed, activity recorded under a free-text tag, or a
									creature the tracker could not identify.
								</p>
								<p class="mt-2 text-xs leading-relaxed text-text-tertiary">
									Its recorded cost and loot still count in Overall. They cannot be assigned to a
									species, so a large unclassified count makes the target comparison less complete.
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
										Realised Net: what this target actually achieved
									</p>
									<p class="mt-1 text-xs leading-relaxed text-text-secondary">
										Loot TT less cycled PED, plus the markup confirmed sales of this species' loot
										have realised, after auction fees.
									</p>
									<p class="mt-2 text-xs leading-relaxed text-text-tertiary">
										It reads the same as TT Net until stock this species produced is sold and the
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
						emptyLabel="No loot recorded for this species yet."
					/>
				{/if}
			</div>
		{/if}
	</div>
</Card>
