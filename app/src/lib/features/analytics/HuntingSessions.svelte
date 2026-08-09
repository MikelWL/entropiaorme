<script lang="ts">
	import Card from '$lib/components/Card.svelte';
	import InfoTip from '$lib/components/InfoTip.svelte';
	import SearchInput from '$lib/components/SearchInput.svelte';
	import StatDisplay from '$lib/components/StatDisplay.svelte';
	import type { TableModel } from '$lib/view/tableModel.svelte';
	import { NO_DATA, formatPed, formatPercent } from '$lib/utils/format';
	import ActivityLootComposition from './ActivityLootComposition.svelte';
	import type { HuntingSessionSection, HuntingSessionSortKey } from './huntingModel.svelte';

	let {
		table,
		selected,
		onselect,
	}: {
		table: TableModel<HuntingSessionSection>;
		selected: HuntingSessionSection | null;
		onselect: (key: string) => void;
	} = $props();

	const SEARCH_THRESHOLD = 8;
	const searchable = $derived(table.filtered.length > SEARCH_THRESHOLD || table.search !== '');
	const displaySections = $derived([
		...table.filtered.filter((section) => !section.isUnassigned),
		...table.filtered.filter((section) => section.isUnassigned),
	]);

	let detailPane = $state<HTMLElement | null>(null);
	$effect(() => {
		void selected?.key;
		if (detailPane) detailPane.scrollTop = 0;
	});

	const signedPed = (value: number) => `${value >= 0 ? '+' : ''}${formatPed(value)}`;
	const netTone = (value: number) => (value >= 0 ? 'text-positive' : 'text-negative');
	const rateTone = (value: number) => netTone(value - 1);
	const COL_NAME = 'min-w-0 flex-[1_1_6rem]';
	const COL_CYCLED = 'min-w-0 flex-[0_1_3.5rem]';
	const COL_MU = 'min-w-0 flex-[0_1_4rem]';
	const COL_REALISED = 'min-w-0 flex-[0_1_7.5rem]';
	const sortArrow = (key: HuntingSessionSortKey) =>
		table.sortKey === key ? (table.sortDir === 'asc' ? '↑' : '↓') : '';
	const sortDescription = (key: HuntingSessionSortKey, label: string) => {
		if (table.sortKey !== key) return `Sort by ${label}`;
		return `Sort by ${label}, currently ${table.sortDir === 'asc' ? 'ascending' : 'descending'}`;
	};
</script>

{#snippet sessionRow(section: HuntingSessionSection, isSelected: boolean)}
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
				class="{COL_NAME} flex min-w-0 items-center gap-1.5 text-sm font-medium tracking-tight
					{section.isUnassigned || section.isArchived ? 'text-text-tertiary' : 'text-text'}"
			>
				<span class="min-w-0 truncate" title={section.name}>{section.name}</span>
				{#if section.isArchived}
					<span
						class="shrink-0 text-[0.625rem] font-medium uppercase tracking-wide text-text-tertiary"
						title="Archived: not offered for play, its history intact"
					>
						Archived
					</span>
				{/if}
			</span>
			{#if section.isUnassigned}
				<span class="sr-only">Session metrics not applicable</span>
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
				<span class="{COL_REALISED} truncate text-right text-xs tabular-nums font-medium {rateTone(section.realisedRate)}">
					{formatPercent(section.realisedRate)}
				</span>
			{/if}
		</button>
	</li>
{/snippet}

<Card class="hover:z-20">
	<div class="grid sm:grid-cols-[46%_minmax(0,1fr)]">
		<div class="min-w-0 border-b border-border/40 sm:border-b-0 sm:border-r">
			<div class="px-2 pt-4">
				{#if searchable}
					<div class="px-3 pb-2">
						<SearchInput bind:value={table.search} placeholder="Find a session" aria-label="Find a session" />
					</div>
				{/if}
				<div class="flex items-center gap-2 rounded-lg border border-transparent px-3 pb-2 text-text-tertiary">
					<button type="button" class="eyebrow {COL_NAME} flex cursor-pointer items-center gap-1 text-left hover:text-text" aria-label={sortDescription('name', 'Session')} onclick={() => table.setSort('name')}>
						Session {#if table.sortKey === 'name'}<span class="text-accent">{sortArrow('name')}</span>{/if}
					</button>
					<button type="button" class="eyebrow {COL_CYCLED} flex cursor-pointer items-center justify-end gap-1 text-right hover:text-text" aria-label={sortDescription('cycled', 'Cycled')} onclick={() => table.setSort('cycled')}>
						Cycled {#if table.sortKey === 'cycled'}<span class="text-accent">{sortArrow('cycled')}</span>{/if}
					</button>
					<button type="button" class="eyebrow {COL_MU} flex cursor-pointer items-center justify-end gap-1 text-right hover:text-text" aria-label={sortDescription('muRate', 'MU Rate')} onclick={() => table.setSort('muRate')}>
						MU Rate {#if table.sortKey === 'muRate'}<span class="text-accent">{sortArrow('muRate')}</span>{/if}
					</button>
					<button type="button" class="eyebrow {COL_REALISED} flex cursor-pointer items-center justify-end gap-1 text-right hover:text-text" aria-label={sortDescription('realisedRate', 'Realised Rate')} onclick={() => table.setSort('realisedRate')}>
						Realised Rate {#if table.sortKey === 'realisedRate'}<span class="text-accent">{sortArrow('realisedRate')}</span>{/if}
					</button>
				</div>
			</div>
			<ul class="flex max-h-[32rem] flex-col gap-1 overflow-y-auto px-2 pb-3">
				{#each displaySections as section (section.key)}
					{@render sessionRow(section, section.key === selected?.key)}
				{/each}
				{#if displaySections.length === 0}
					<li class="px-3 py-4 text-center text-xs text-text-tertiary">No session matches that search.</li>
				{/if}
			</ul>
		</div>

		{#if selected}
			<div bind:this={detailPane} class="min-w-0 max-h-[32rem] overflow-y-auto p-5">
				{#if selected.isUnassigned}
					<div class="flex min-h-28 items-center justify-center">
						<div class="flex items-center gap-1.5 text-sm text-text-secondary">
							<span>Hunting outside a defined session cannot join the routine comparison.</span>
							<InfoTip label="What unassigned hunting means" width="w-80">
								<p class="text-xs font-semibold leading-relaxed text-text">Hunting without a repeatable routine</p>
								<p class="mt-1 text-xs leading-relaxed text-text-secondary">Its cost and loot still count in Overall. Without a session definition there is no deliberate activity to rank, so this diagnostic row carries no economic claim of its own.</p>
							</InfoTip>
						</div>
					</div>
				{:else}
					<div class="grid grid-cols-3 gap-x-5">
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
					</div>
					<ActivityLootComposition items={selected.items} marketAvailable={selected.muProjectedReturns !== null} emptyLabel="No loot recorded for this session yet." />
				{/if}
			</div>
		{/if}
	</div>
</Card>
