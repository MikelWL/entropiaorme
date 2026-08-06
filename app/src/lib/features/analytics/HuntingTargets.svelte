<script lang="ts">
	/**
	 * The observed Targets axis: mob species compared on the same frame,
	 * columns, and market treatment as the Tree Cutting sub-activities, so
	 * a player who knows one tab is already at home in the other. Maturity
	 * is a drilldown of the selected species, never a peer row: the species
	 * is the decision unit a player deliberately repeats.
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
	import { confidenceTip, confidenceTitle, markupLabel } from './marketConfidence';
	import type { HuntingTargetSection, HuntingTargetSortKey } from './huntingModel.svelte';
	import type { TreeCuttingItem } from './treeCuttingModel.svelte';

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

	// The maturity drilldown's own diagnostic band sits last whatever its
	// kill count, exactly as the buckets above it do.
	const orderedMaturities = $derived.by(() => {
		if (!selected) return [];
		return [
			...selected.maturities.filter((band) => band.maturity !== ''),
			...selected.maturities.filter((band) => band.maturity === ''),
		];
	});

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

{#snippet confidenceBody(item: TreeCuttingItem)}
	{@const tip = confidenceTip(item)}
	<p class="text-xs font-semibold leading-relaxed text-text">{tip.title}</p>
	<p class="mt-1 text-xs leading-relaxed text-text-secondary">{tip.subtitle}</p>
	{#if tip.example}
		<p class="mt-2 text-xs leading-relaxed text-text-secondary">{tip.example}</p>
	{/if}
	{#if tip.note}
		<p class="mt-2 text-xs leading-relaxed text-text-tertiary">{tip.note}</p>
	{/if}
{/snippet}

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
							<span>
								{selected.kills}
								{selected.kills === 1 ? 'kill is' : 'kills are'} unclassified and cannot be assigned
								to a species.
							</span>
							<InfoTip label="Why kills can be unclassified" width="w-80">
								<p class="text-xs font-semibold leading-relaxed text-text">
									Why kills can be unclassified
								</p>
								<p class="mt-1 text-xs leading-relaxed text-text-secondary">
									A kill is unclassified when no species was recorded for it: sessions tracked
									before species stamping existed, kills recorded under a free-text tag, or a
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

					<div class="mt-4 grid grid-cols-3 gap-x-5">
						<StatDisplay
							label="Kills"
							value={String(selected.kills)}
							emphasis="secondary"
						/>
						<StatDisplay
							label="Net / Kill"
							value={selected.kills > 0
								? signedPed((selected.returns - selected.cycled) / selected.kills)
								: NO_DATA}
							unit={selected.kills > 0 ? 'PED' : ''}
							emphasis="secondary"
						>
							{#snippet labelSuffix()}
								<InfoTip align="right" width="w-80" label="What Net / Kill covers">
									<p class="text-xs font-semibold leading-relaxed text-text">
										Direct cost per kill only
									</p>
									<p class="mt-1 text-xs leading-relaxed text-text-secondary">
										Weapon and enhancer decay attributed to kills of this species. Heal and
										armour are recorded per session, not per kill, so a full per-kill cost would
										be a guess; the Dashboard and Overview carry the whole session's economics.
									</p>
								</InfoTip>
							{/snippet}
						</StatDisplay>
						<StatDisplay
							label="PES/100"
							value={selected.pesPer100Ped !== null
								? selected.pesPer100Ped.toFixed(2)
								: NO_DATA}
							emphasis="secondary"
						>
							{#snippet labelSuffix()}
								<InfoTip align="right" width="w-80" label="How PES is attributed to a species">
									<p class="text-xs font-semibold leading-relaxed text-text">
										Skill progress a species can claim
									</p>
									<p class="mt-1 text-xs leading-relaxed text-text-secondary">
										Skill gains are recorded per session, not per kill, so a species may claim a
										session's skill total only when its kills dominated that session.
									</p>
									<p class="mt-2 text-xs leading-relaxed text-text-tertiary">
										{#if selected.pesSessions > 0}
											This figure comes from {selected.pesSessions}
											{selected.pesSessions === 1 ? 'session' : 'sessions'} this species dominated;
											mixed hunts contribute nothing rather than being guessed at.
										{:else}
											No session was dominated by this species, so no skill claim is made rather
											than one being guessed at.
										{/if}
									</p>
								</InfoTip>
							{/snippet}
						</StatDisplay>
					</div>

					{#if selected.maturities.length > 1 || (selected.maturities.length === 1 && selected.maturities[0].maturity !== '')}
						<div class="mt-5 border-t border-border/50 pt-4">
							<div
								class="sticky top-0 z-10 -mx-5 flex items-center gap-3 bg-surface px-[1.875rem] py-1 text-text-tertiary"
							>
								<span class="eyebrow flex-1 min-w-0">Maturity</span>
								<span class="eyebrow w-16 text-right shrink-0">Kills</span>
								<span class="eyebrow w-20 text-right shrink-0">Cycled</span>
								<span class="eyebrow w-20 text-right shrink-0">TT Rate</span>
							</div>
							<ul class="flex flex-col gap-1">
								{#each orderedMaturities as band (band.maturity)}
									<li
										class="flex items-center gap-3 rounded-md px-2.5 py-2 border border-transparent
											hover:bg-surface-hover/30 hover:border-border/40
											transition-[background-color,border-color] duration-[var(--duration-base)] ease-[var(--ease-out)]"
									>
										<span
											class="flex-1 min-w-0 flex items-center gap-1.5 text-sm font-medium tracking-tight
												{band.maturity === '' ? 'text-text-tertiary' : 'text-text'}"
										>
											<span class="min-w-0 truncate">
												{band.maturity === '' ? 'Unrecorded' : band.maturity}
											</span>
											{#if band.maturity === ''}
												<InfoTip label="Why a kill can lack a maturity" width="w-80">
													<p class="text-xs font-semibold leading-relaxed text-text">
														Kills without a maturity band
													</p>
													<p class="mt-1 text-xs leading-relaxed text-text-secondary">
														The tracker recorded the species but never learned the creature's
														maturity: an unread nameplate, or a session from before maturity
														stamping existed.
													</p>
													<p class="mt-2 text-xs leading-relaxed text-text-tertiary">
														Their cost and loot still count for the species; they simply cannot
														join a band, so a large unrecorded count makes this drilldown less
														complete.
													</p>
												</InfoTip>
											{/if}
										</span>
										<span class="w-16 shrink-0 text-right text-sm tabular-nums text-text">
											{band.kills}
										</span>
										<span class="w-20 shrink-0 text-right text-sm tabular-nums text-text">
											{formatPed(band.cycled)}
										</span>
										<span
											class="w-20 shrink-0 text-right text-sm tabular-nums font-medium {rateTone(
												band.lootRate,
											)}"
										>
											{formatPercent(band.lootRate)}
										</span>
									</li>
								{/each}
							</ul>
						</div>
					{/if}

					{#if selected.items.length > 0}
						<div class="mt-5 border-t border-border/50 pt-4">
							<div
								class="sticky top-0 z-10 -mx-5 flex items-center gap-3 bg-surface px-[1.875rem] py-1 text-text-tertiary"
							>
								<span class="eyebrow flex-1 min-w-0">Item</span>
								<span class="eyebrow w-20 text-right shrink-0">TT</span>
								<span class="eyebrow w-14 text-right shrink-0">Share</span>
								<span class="eyebrow w-20 text-right shrink-0">Markup</span>
								<span class="eyebrow w-12 text-center shrink-0">Conf</span>
							</div>

							<ul class="flex flex-col gap-1">
								{#each selected.items as item (item.name)}
									<li
										class="flex items-center gap-3 rounded-md px-2.5 py-2 border border-transparent
											hover:bg-surface-hover/30 hover:border-border/40
											transition-[background-color,border-color] duration-[var(--duration-base)] ease-[var(--ease-out)]"
									>
										<span class="flex-1 min-w-0 truncate text-sm font-medium tracking-tight text-text">
											{item.name}
										</span>

										<span class="text-sm tabular-nums font-medium text-text shrink-0 w-20 text-right">
											{formatPed(item.ttValue)}
										</span>

										<span
											class="text-sm tabular-nums font-semibold text-accent shrink-0 w-14 text-right tracking-tight"
										>
											{item.sharePct.toFixed(1)}%
										</span>

										<div class="w-20 shrink-0 flex items-center justify-end">
											{#if selected.muProjectedReturns === null}
												<span class="text-sm text-text-tertiary">{NO_DATA}</span>
											{:else}
												<span
													class="inline-flex h-5 flex-col items-end justify-center tabular-nums"
													aria-label={markupLabel(item)}
												>
													{#if item.floored && item.ownMarkupPct !== null}
														{@const observedMarkup = item.ownMarkupPct}
														<span class="text-[9px] leading-[9px] text-text-tertiary line-through">
															{formatPercent(observedMarkup / 100)}
														</span>
														<span class="text-xs leading-[11px] text-text-secondary">
															{formatPercent(item.effectiveMarkupPct / 100)}
														</span>
													{:else}
														<span class="text-sm leading-5 text-text-secondary">
															{formatPercent(item.effectiveMarkupPct / 100)}
														</span>
													{/if}
												</span>
											{/if}
										</div>

										<div class="w-12 shrink-0 flex items-center justify-center">
											{#if selected.muProjectedReturns === null}
												<span class="text-sm text-text-tertiary">{NO_DATA}</span>
											{:else}
												<InfoTip align="right" width="w-96" label={confidenceTitle(item.tier)}>
													{#snippet trigger()}
														{#if item.tier === 'liquid'}
															<span class="text-positive" aria-label="High volume">✓</span>
														{:else if item.tier === 'middling'}
															<span class="text-warning" aria-label="Medium volume">⚠</span>
														{:else}
															<span class="text-error font-semibold" aria-label="Low volume">!</span>
														{/if}
													{/snippet}
													{@render confidenceBody(item)}
												</InfoTip>
											{/if}
										</div>
									</li>
								{/each}
							</ul>
						</div>
					{:else}
						<p class="mt-4 text-xs text-text-tertiary px-2.5">
							No loot recorded for this species yet.
						</p>
					{/if}
				{/if}
			</div>
		{/if}
	</div>
</Card>
