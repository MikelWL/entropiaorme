<script lang="ts">
	/**
	 * The designated Sessions axis: one row per session definition, keyed by
	 * the definition and never by recorded free text, on the same two-pane
	 * frame as every sibling panel. Selecting a definition opens what the
	 * routine is made of: its headline economics, the activity signatures
	 * declared while it was played (quest families with variant drilldown,
	 * deliberate co-activation bundles as one joint-return unit, named
	 * segments, and the unscoped remainder), its mob composition, and its
	 * recent instances with a trend read.
	 *
	 * Quest-shaped rows carry the fixed-reward break-even readout as a
	 * visible column, with the worked explanation behind an InfoTip. Skill
	 * rewards are PES and never enter a liquid figure.
	 *
	 * The unassigned bucket is pinned last and unranked, but its figures
	 * SHOW: unlike an unclassified kill, an unassigned session has complete,
	 * honest numbers; it merely belongs to no routine yet.
	 */
	import Card from '$lib/components/Card.svelte';
	import InfoTip from '$lib/components/InfoTip.svelte';
	import SearchInput from '$lib/components/SearchInput.svelte';
	import StatDisplay from '$lib/components/StatDisplay.svelte';
	import type { HuntingSignature } from '$lib/types/analytics';
	import type { TableModel } from '$lib/view/tableModel.svelte';
	import { NO_DATA, formatPed, formatPercent } from '$lib/utils/format';
	import {
		type HuntingSessionSection,
		type HuntingSessionSortKey,
		signatureEconomics,
	} from './huntingModel.svelte';

	let {
		table,
		selected,
		onselect,
	}: {
		table: TableModel<HuntingSessionSection>;
		selected: HuntingSessionSection | null;
		onselect: (key: string) => void;
	} = $props();

	// Definitions accumulate over a playing career; the search appears once
	// scanning stops being quicker than typing, and stays visible while a
	// query is live so a filter can always be seen and cleared.
	const SEARCH_THRESHOLD = 8;
	const searchable = $derived(table.filtered.length > SEARCH_THRESHOLD || table.search !== '');

	// The unassigned bucket is pinned after the deliberate definitions
	// whatever the sort: it is not a routine, so it holds no rank.
	let displaySections = $derived([
		...table.filtered.filter((section) => !section.isUnassigned),
		...table.filtered.filter((section) => section.isUnassigned),
	]);

	// Which family rows are expanded to their variants, keyed by kind and
	// label so a segment sharing a family's name cannot collide; the set
	// resets when the selected definition changes.
	let expandedFamilies = $state<Set<string>>(new Set());
	let expandedFor = $state<string | null>(null);
	$effect(() => {
		if (selected?.key !== expandedFor) {
			expandedFor = selected?.key ?? null;
			expandedFamilies = new Set();
		}
	});
	const familyKey = (row: HuntingSignature) => `${row.kind}:${row.label}`;
	function toggleFamily(row: HuntingSignature) {
		const next = new Set(expandedFamilies);
		if (next.has(familyKey(row))) {
			next.delete(familyKey(row));
		} else {
			next.add(familyKey(row));
		}
		expandedFamilies = next;
	}

	const signedPed = (value: number) => `${value >= 0 ? '+' : ''}${formatPed(value)}`;
	const netTone = (value: number) => (value >= 0 ? 'text-positive' : 'text-negative');
	const formatHours = (hours: number) => {
		if (hours >= 10) return `${hours.toFixed(0)}h`;
		if (hours >= 1) return `${hours.toFixed(1)}h`;
		return `${Math.round(hours * 60)}m`;
	};
	// The family's ledger-date voice ("Aug 5"), gaining a year once a row
	// falls outside the current one: instances are read historically.
	const instanceDate = (startedAt: number) => {
		const date = new Date(startedAt * 1000);
		const withYear = date.getFullYear() !== new Date().getFullYear();
		return date.toLocaleDateString('en-US', {
			month: 'short',
			day: 'numeric',
			...(withYear ? { year: 'numeric' } : {}),
		});
	};

	const TREND_LABEL = {
		improving: 'Improving ↗',
		declining: 'Declining ↘',
		stable: 'Stable →',
	} as const;

	const KIND_LABEL: Record<string, string> = {
		quest_family: 'Family',
		quest: 'Quest',
		bundle: 'Bundle',
		segment: 'Segment',
		ambient: '',
	};

	/** The visible break-even column: net after the configured liquid
	 * reward over the recorded runs. `null` renders as no-data. */
	function afterReward(row: HuntingSignature): number | null {
		if (row.rewardIsSkill || row.rewardPed == null || row.runs <= 0) return null;
		return row.returns - row.cycled + row.rewardPed * row.runs;
	}
	const signaturePesPer100 = (row: HuntingSignature) =>
		row.cycled > 0 ? ((row.pes / row.cycled) * 100).toFixed(2) : NO_DATA;

	// The list's column widths, declared once because the header and the rows
	// have to shrink identically or they stop lining up. Kept in step with the
	// sibling panes so the toggle reads as one surface.
	const COL_NAME = 'min-w-0 flex-[1_1_6rem]';
	const COL_CYCLED = 'min-w-0 flex-[0_1_3.5rem]';
	const COL_RATE = 'min-w-0 flex-[0_1_4rem]';
	const COL_PES = 'min-w-0 flex-[0_1_7.5rem]';
	const sortArrow = (key: HuntingSessionSortKey) =>
		table.sortKey === key ? (table.sortDir === 'asc' ? '↑' : '↓') : '';
	const sortDescription = (key: HuntingSessionSortKey, label: string) => {
		if (table.sortKey !== key) return `Sort by ${label}`;
		return `Sort by ${label}, currently ${table.sortDir === 'asc' ? 'ascending' : 'descending'}`;
	};
</script>

{#snippet directCostTip()}
	<InfoTip align="right" width="w-80" label="What direct figures cover">
		<p class="text-xs font-semibold leading-relaxed text-text">Direct hunting cost only</p>
		<p class="mt-1 text-xs leading-relaxed text-text-secondary">
			Weapon and enhancer decay attributed to kills. Heal and armour are recorded per session,
			not per kill, so a session's full sustainability reads on the Dashboard and Overview.
		</p>
	</InfoTip>
{/snippet}

{#snippet economicsTip(row: HuntingSignature)}
	{@const economics = signatureEconomics(row)}
	{@const variantsHaveRewards =
		row.kind === 'quest_family' && row.variants.some((variant) => variant.rewardPed != null)}
	<p class="text-xs font-semibold leading-relaxed text-text">
		Fixed-reward break-even for {row.label}
	</p>
	{#if economics.shortfallPerRun !== null}
		<p class="mt-1 text-xs leading-relaxed text-text-secondary">
			Across {row.runs}
			{row.runs === 1 ? 'run' : 'runs'}, one run
			{economics.shortfallPerRun >= 0
				? `costs about ${formatPed(economics.shortfallPerRun)} PED before its reward`
				: `already returns ${formatPed(-economics.shortfallPerRun)} PED before any reward`}.
			A run is a recorded focus stretch, not a confirmed completion.
		</p>
	{:else}
		<p class="mt-1 text-xs leading-relaxed text-text-secondary">
			No recorded runs yet, so there is no measured cost to hold the reward against.
		</p>
	{/if}
	{#if economics.rewardIsSkill}
		<p class="mt-2 text-xs leading-relaxed text-text-secondary">
			The configured reward is a skill reward: it counts as PES, never as liquid return, so no
			liquid break-even applies.
		</p>
	{:else if economics.rewardPed !== null}
		<p class="mt-2 text-xs leading-relaxed text-text-secondary">
			The configured reward is {formatPed(economics.rewardPed)} PED per completion{economics.netAfterRewardPerRun !==
			null
				? `, putting a completed run at ${signedPed(economics.netAfterRewardPerRun)} PED`
				: ''}.
		</p>
		{#if economics.voucherScenarioPerRun !== null && row.expectedRewardMarkupPercent != null}
			<p class="mt-2 text-xs leading-relaxed text-text-tertiary">
				If the reward sold at its estimated {formatPercent(row.expectedRewardMarkupPercent / 100)}
				markup, a completed run would read {signedPed(economics.voucherScenarioPerRun)} PED. That is
				an informational scenario only; nothing is realised until a sale confirms it.
			</p>
		{/if}
	{:else if variantsHaveRewards}
		<p class="mt-2 text-xs leading-relaxed text-text-secondary">
			The variants in this family carry different rewards, so no single family figure would be
			honest. Expand the family to read each variant's own break-even.
		</p>
	{:else}
		<p class="mt-2 text-xs leading-relaxed text-text-tertiary">
			No liquid reward is configured for this quest. Set one on the Quests page to complete the
			break-even readout.
		</p>
	{/if}
{/snippet}

{#snippet signatureRow(row: HuntingSignature, depth: number)}
	{@const isAmbient = row.kind === 'ambient'}
	{@const isFamily = row.kind === 'quest_family'}
	{@const questShaped = row.kind === 'quest' || isFamily}
	{@const net = row.returns - row.cycled}
	{@const rewarded = afterReward(row)}
	<li
		class="flex items-center gap-3 rounded-md px-2.5 py-2 border border-transparent
			hover:bg-surface-hover/30 hover:border-border/40
			transition-[background-color,border-color] duration-[var(--duration-base)] ease-[var(--ease-out)]
			{depth > 0 ? 'ml-5' : ''}"
	>
		<span class="flex-1 min-w-0 flex items-center gap-1.5">
			{#if isFamily}
				<button
					type="button"
					class="shrink-0 -m-1 p-1 cursor-pointer text-text-tertiary transition-colors duration-[var(--duration-fast)] hover:text-text"
					aria-expanded={expandedFamilies.has(familyKey(row))}
					aria-label={expandedFamilies.has(familyKey(row))
						? `Collapse ${row.label} variants`
						: `Expand ${row.label} variants`}
					onclick={() => toggleFamily(row)}
				>
					<span
						class="inline-block text-[0.625rem] transition-transform duration-[var(--duration-fast)]
							{expandedFamilies.has(familyKey(row)) ? 'rotate-90' : ''}"
					>
						▶
					</span>
				</button>
			{/if}
			<span
				class="min-w-0 truncate text-sm font-medium tracking-tight
					{isAmbient ? 'text-text-tertiary' : 'text-text'}"
				title={row.label}
			>
				{row.label}
			</span>
			{#if KIND_LABEL[row.kind]}
				<span class="shrink-0 text-[0.625rem] font-medium uppercase tracking-wide text-text-tertiary">
					{KIND_LABEL[row.kind]}
				</span>
			{/if}
			{#if isAmbient}
				<InfoTip label="What the unscoped remainder is" width="w-80">
					<p class="text-xs font-semibold leading-relaxed text-text">Play with nothing declared</p>
					<p class="mt-1 text-xs leading-relaxed text-text-secondary">
						Kills, cost, and skill recorded while no quest or segment was standing, plus events
						from sessions tracked before activities could be declared at all.
					</p>
					<p class="mt-2 text-xs leading-relaxed text-text-tertiary">
						It is a remainder, not a ranked activity: it keeps the rows above honest by holding
						everything they may not claim.
					</p>
				</InfoTip>
			{/if}
			{#if questShaped}
				<InfoTip label={`Break-even for ${row.label}`} width="w-96">
					{@render economicsTip(row)}
				</InfoTip>
			{/if}
		</span>

		<span class="w-10 shrink-0 text-right text-xs tabular-nums text-text-secondary">
			{isAmbient ? NO_DATA : row.runs}
		</span>
		<span class="w-14 shrink-0 text-right text-xs tabular-nums text-text-secondary">
			{signaturePesPer100(row)}
		</span>
		<span class="w-16 shrink-0 text-right text-xs tabular-nums text-text">
			{formatPed(row.cycled)}
		</span>
		<span class="w-16 shrink-0 text-right text-xs tabular-nums text-text">
			{signedPed(net)}
		</span>
		<span class="w-[4.5rem] shrink-0 text-right text-xs tabular-nums font-medium">
			{#if rewarded !== null}
				<span class={netTone(rewarded)}>{signedPed(rewarded)}</span>
			{:else if row.rewardIsSkill}
				<span class="text-text-tertiary" title="The reward is skill progress: PES, never liquid">
					PES
				</span>
			{:else}
				<span class="text-text-tertiary">{NO_DATA}</span>
			{/if}
		</span>
	</li>
	{#if isFamily && expandedFamilies.has(familyKey(row))}
		{#each row.variants as variant (variant.label)}
			{@render signatureRow(variant, depth + 1)}
		{/each}
	{/if}
{/snippet}

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
			<span class="{COL_CYCLED} truncate text-right text-xs tabular-nums text-text">
				{formatPed(section.cycled)}
			</span>
			<span class="{COL_RATE} truncate text-right text-xs tabular-nums text-text">
				{formatPercent(section.lootRate)}
			</span>
			<span class="{COL_PES} truncate text-right text-xs tabular-nums font-medium text-text">
				{section.pesPer100Ped.toFixed(2)}
			</span>
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
							placeholder="Find a session"
							aria-label="Find a session"
						/>
					</div>
				{/if}
				<div
					class="flex items-center gap-2 rounded-lg border border-transparent px-3 pb-2 text-text-tertiary"
				>
					<button
						type="button"
						class="eyebrow {COL_NAME} flex cursor-pointer items-center gap-1 text-left transition-colors duration-[var(--duration-fast)] hover:text-text"
						aria-label={sortDescription('name', 'Session')}
						onclick={() => table.setSort('name')}
					>
						Session
						{#if table.sortKey === 'name'}<span class="text-accent">{sortArrow('name')}</span>{/if}
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
						class="eyebrow {COL_RATE} flex cursor-pointer items-center justify-end gap-1 text-right transition-colors duration-[var(--duration-fast)] hover:text-text"
						aria-label={sortDescription('lootRate', 'TT Rate')}
						onclick={() => table.setSort('lootRate')}
					>
						TT Rate
						{#if table.sortKey === 'lootRate'}<span class="text-accent">{sortArrow('lootRate')}</span>{/if}
					</button>
					<button
						type="button"
						class="eyebrow {COL_PES} flex cursor-pointer items-center justify-end gap-1 text-right transition-colors duration-[var(--duration-fast)] hover:text-text"
						aria-label={sortDescription('pesPer100Ped', 'PES per 100 PED')}
						onclick={() => table.setSort('pesPer100Ped')}
					>
						PES/100
						{#if table.sortKey === 'pesPer100Ped'}<span class="text-accent"
								>{sortArrow('pesPer100Ped')}</span
							>{/if}
					</button>
				</div>
			</div>
			<ul class="flex max-h-[32rem] flex-col gap-1 overflow-y-auto px-2 pb-3">
				{#each displaySections as section (section.key)}
					{@render sessionRow(section, section.key === selected?.key)}
				{/each}
				{#if displaySections.length === 0}
					<li class="px-3 py-4 text-center text-xs text-text-tertiary">
						No session matches that search.
					</li>
				{/if}
			</ul>
		</div>

		{#if selected}
			<!-- One scroll region bounded to the list pane's own height, so the
				two sides of the hairline stay the same height and the pane never
				stacks nested scrollers. -->
			<div class="min-w-0 max-h-[32rem] overflow-y-auto p-5">
				{#if selected.isUnassigned}
					<div class="mb-4 flex items-start gap-1.5 text-sm text-text-secondary">
						<span>
							{selected.instances}
							{selected.instances === 1 ? 'session was' : 'sessions were'} recorded outside any
							definition.
						</span>
						<InfoTip label="What unassigned sessions are" width="w-80">
							<p class="text-xs font-semibold leading-relaxed text-text">
								Sessions without a definition
							</p>
							<p class="mt-1 text-xs leading-relaxed text-text-secondary">
								Sessions tracked before definitions existed, or deliberately started outside one.
								Their figures are complete and honest; they simply belong to no routine, so they
								hold no rank in the comparison.
							</p>
							<p class="mt-2 text-xs leading-relaxed text-text-tertiary">
								An ended session can be moved into a definition from the dashboard's Review
								surface, after which it counts toward that routine here.
							</p>
						</InfoTip>
					</div>
				{/if}

				<div class="grid grid-cols-3 gap-x-5 gap-y-4">
					<StatDisplay label="TT Net" value={signedPed(selected.ttNet)} unit="PED">
						{#snippet labelSuffix()}
							{@render directCostTip()}
						{/snippet}
					</StatDisplay>
					<StatDisplay label="TT Rate" value={formatPercent(selected.lootRate)} />
					<StatDisplay label="PES" value={selected.pes.toFixed(2)} />
					<StatDisplay
						label="Instances"
						value={String(selected.instances)}
						emphasis="secondary"
					/>
					<StatDisplay
						label="Kills"
						value={String(selected.kills)}
						emphasis="secondary"
					/>
					<StatDisplay
						label="Duration"
						value={formatHours(selected.durationHours)}
						emphasis="secondary"
					/>
				</div>

				{#if selected.activities.length > 0}
					<div class="mt-5 border-t border-border/50 pt-4">
						<div class="flex items-center gap-3 px-2.5 pb-1 text-text-tertiary">
							<span class="eyebrow flex-1 min-w-0">Activity</span>
							<span class="eyebrow w-10 text-right shrink-0">Runs</span>
							<span class="eyebrow w-14 text-right shrink-0">PES/100</span>
							<span class="eyebrow w-16 text-right shrink-0">Cycled</span>
							<span class="eyebrow w-16 text-right shrink-0">TT Net</span>
							<span class="eyebrow w-[4.5rem] text-right shrink-0">+ Reward</span>
						</div>
						<ul class="flex flex-col gap-1">
							{#each selected.activities as row (row.kind + row.label)}
								{@render signatureRow(row, 0)}
							{/each}
						</ul>
					</div>
				{/if}

				{#if selected.mobs.length > 0}
					{@const mobLootTotal = selected.mobs.reduce((sum, mob) => sum + mob.lootTt, 0)}
					<div class="mt-5 border-t border-border/50 pt-4">
						<div class="flex items-center gap-3 px-2.5 pb-1 text-text-tertiary">
							<span class="eyebrow flex-1 min-w-0">Mob</span>
							<span class="eyebrow w-16 text-right shrink-0">Kills</span>
							<span class="eyebrow w-20 text-right shrink-0">Loot TT</span>
							<span class="eyebrow w-14 text-right shrink-0">Share</span>
						</div>
						<ul class="flex flex-col gap-1">
							{#each selected.mobs as mob (mob.mobSpecies)}
								<li
									class="flex items-center gap-3 rounded-md px-2.5 py-2 border border-transparent
										hover:bg-surface-hover/30 hover:border-border/40
										transition-[background-color,border-color] duration-[var(--duration-base)] ease-[var(--ease-out)]"
								>
									<span
										class="flex-1 min-w-0 truncate text-sm font-medium tracking-tight text-text"
										title={mob.mobSpecies}
									>
										{mob.mobSpecies}
									</span>
									<span class="w-16 shrink-0 text-right text-sm tabular-nums text-text">
										{mob.kills}
									</span>
									<span class="w-20 shrink-0 text-right text-sm tabular-nums text-text">
										{formatPed(mob.lootTt)}
									</span>
									<span
										class="w-14 shrink-0 text-right text-sm tabular-nums font-semibold text-accent tracking-tight"
									>
										{mobLootTotal > 0 ? `${((mob.lootTt / mobLootTotal) * 100).toFixed(1)}%` : NO_DATA}
									</span>
								</li>
							{/each}
						</ul>
					</div>
				{/if}

				{#if selected.instanceRows.length > 0}
					<div class="mt-5 border-t border-border/50 pt-4">
						<div class="flex items-center gap-3 px-2.5 pb-1 text-text-tertiary">
							<span class="eyebrow flex-1 min-w-0 flex items-center gap-2">
								Recent instances
								{#if selected.instances > selected.instanceRows.length}
									<span class="text-[0.625rem] font-normal normal-case tracking-normal">
										showing {selected.instanceRows.length} of {selected.instances}
									</span>
								{/if}
								{#if selected.trend}
									{@const trend = selected.trend}
									<InfoTip label="How the trend is read" width="w-80">
										{#snippet trigger()}
											<span
												class="text-[0.625rem] font-medium uppercase tracking-wide text-text-secondary
													border-b border-dotted border-border/70"
											>
												{TREND_LABEL[trend]}
											</span>
										{/snippet}
										<p class="text-xs font-semibold leading-relaxed text-text">
											The newer half against the older half
										</p>
										<p class="mt-1 text-xs leading-relaxed text-text-secondary">
											The TT loot rate of the newer half of these instances compared with the older
											half; within two percentage points reads as stable. Based on
											{selected.instanceRows.length} instances.
										</p>
										<p class="mt-2 text-xs leading-relaxed text-text-tertiary">
											Loot is the noisiest series in the game: one large loot can carry a whole
											half. Treat this as a nudge to look at the instances, never as a verdict.
										</p>
									</InfoTip>
								{/if}
							</span>
							<span class="eyebrow w-14 text-right shrink-0">Time</span>
							<span class="eyebrow w-14 text-right shrink-0">Kills</span>
							<span class="eyebrow w-20 text-right shrink-0">Cycled</span>
							<span class="eyebrow w-20 text-right shrink-0">TT Net</span>
						</div>
						<ul class="flex flex-col gap-1">
							{#each selected.instanceRows as instance (instance.sessionId)}
								{@const instanceNet = instance.returns - instance.cycled}
								<li
									class="flex items-center gap-3 rounded-md px-2.5 py-2 border border-transparent
										hover:bg-surface-hover/30 hover:border-border/40
										transition-[background-color,border-color] duration-[var(--duration-base)] ease-[var(--ease-out)]"
								>
									<span class="flex-1 min-w-0 truncate text-sm tabular-nums text-text-secondary">
										{instanceDate(instance.startedAt)}
									</span>
									<span class="w-14 shrink-0 text-right text-sm tabular-nums text-text-secondary">
										{formatHours(instance.durationHours)}
									</span>
									<span class="w-14 shrink-0 text-right text-sm tabular-nums text-text">
										{instance.kills}
									</span>
									<span class="w-20 shrink-0 text-right text-sm tabular-nums text-text">
										{formatPed(instance.cycled)}
									</span>
									<span class="w-20 shrink-0 text-right text-sm tabular-nums font-medium text-text">
										{signedPed(instanceNet)}
									</span>
								</li>
							{/each}
						</ul>
					</div>
				{/if}
			</div>
		{/if}
	</div>
</Card>
