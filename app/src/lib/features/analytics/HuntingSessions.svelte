<script lang="ts">
	/**
	 * The designated Sessions axis: one row per session definition, keyed by
	 * the definition and never by recorded free text, on the same two-pane
	 * frame as every sibling panel. Selecting a definition opens what the
	 * routine is made of: its headline economics, the activity signatures
	 * declared while it was played (quest families with variant drilldown,
	 * deliberate co-activation bundles as one joint-return unit, named
	 * segments, and the unscoped remainder), its mob composition, and its
	 * recorded instances with a trend read.
	 *
	 * Quest-shaped rows carry the fixed-reward break-even readout: what a
	 * run costs before its reward, against the configured liquid reward,
	 * with the voucher-markup scenario kept informational. Skill rewards
	 * are PES and never enter a liquid figure.
	 */
	import Card from '$lib/components/Card.svelte';
	import InfoTip from '$lib/components/InfoTip.svelte';
	import Input from '$lib/components/Input.svelte';
	import StatDisplay from '$lib/components/StatDisplay.svelte';
	import type { HuntingSignature } from '$lib/types/analytics';
	import type { SortDir, SortKey } from '$lib/view/tableModel.svelte';
	import { NO_DATA, formatPed, formatPercent } from '$lib/utils/format';
	import {
		type HuntingSessionSection,
		type HuntingSessionSortKey,
		signatureEconomics,
	} from './huntingModel.svelte';

	let {
		sections,
		selected,
		onselect,
		sortKey,
		sortDir,
		onsort,
	}: {
		sections: HuntingSessionSection[];
		selected: HuntingSessionSection | null;
		onselect: (key: string) => void;
		sortKey: SortKey<HuntingSessionSection> | undefined;
		sortDir: SortDir;
		onsort: (key: HuntingSessionSortKey) => void;
	} = $props();

	// Definitions accumulate over a playing career; the search appears once
	// scanning stops being quicker than typing.
	const SEARCH_THRESHOLD = 8;
	let query = $state('');
	const searchable = $derived(sections.length > SEARCH_THRESHOLD);
	const matches = $derived(
		query.trim() === ''
			? sections
			: sections.filter((section) =>
					section.name.toLowerCase().includes(query.trim().toLowerCase()),
				),
	);

	// The unassigned bucket is pinned after the deliberate definitions
	// whatever the sort: a diagnostic bucket has no rank to take part in.
	let displaySections = $derived([
		...matches.filter((section) => !section.isUnassigned),
		...matches.filter((section) => section.isUnassigned),
	]);

	// Which family rows are expanded to their variants. Keyed by label:
	// stable within one definition's activity list.
	let expandedFamilies = $state<Set<string>>(new Set());
	function toggleFamily(label: string) {
		const next = new Set(expandedFamilies);
		if (next.has(label)) {
			next.delete(label);
		} else {
			next.add(label);
		}
		expandedFamilies = next;
	}

	const signedPed = (value: number) => `${value >= 0 ? '+' : ''}${formatPed(value)}`;
	const netTone = (value: number) => (value >= 0 ? 'text-positive' : 'text-negative');
	const rateTone = (value: number) => netTone(value - 1);
	const formatHours = (hours: number) => {
		if (hours >= 10) return `${hours.toFixed(0)}h`;
		if (hours >= 1) return `${hours.toFixed(1)}h`;
		return `${Math.round(hours * 60)}m`;
	};
	const instanceDate = (startedAt: number) =>
		new Date(startedAt * 1000).toLocaleDateString(undefined, {
			day: 'numeric',
			month: 'short',
		});

	const TREND_LABEL = {
		improving: 'Improving',
		declining: 'Declining',
		stable: 'Stable',
	} as const;
	const TREND_TONE = {
		improving: 'text-positive',
		declining: 'text-negative',
		stable: 'text-text-tertiary',
	} as const;

	const KIND_LABEL: Record<string, string> = {
		quest_family: 'Family',
		quest: 'Quest',
		bundle: 'Bundle',
		segment: 'Segment',
		ambient: '',
	};

	// The list's column widths, declared once because the header and the rows
	// have to shrink identically or they stop lining up. Kept in step with the
	// sibling panes so the toggle reads as one surface.
	const COL_NAME = 'min-w-0 flex-[1_1_6rem]';
	const COL_CYCLED = 'min-w-0 flex-[0_1_3.5rem]';
	const COL_RATE = 'min-w-0 flex-[0_1_4rem]';
	const COL_PES = 'min-w-0 flex-[0_1_7.5rem]';
	const sortArrow = (key: HuntingSessionSortKey) =>
		sortKey === key ? (sortDir === 'asc' ? '↑' : '↓') : '';
	const sortDescription = (key: HuntingSessionSortKey, label: string) => {
		if (sortKey !== key) return `Sort by ${label}`;
		return `Sort by ${label}, currently ${sortDir === 'asc' ? 'ascending' : 'descending'}`;
	};
</script>

{#snippet economicsTip(row: HuntingSignature)}
	{@const economics = signatureEconomics(row)}
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
					class="shrink-0 cursor-pointer text-text-tertiary transition-colors duration-[var(--duration-fast)] hover:text-text"
					aria-expanded={expandedFamilies.has(row.label)}
					aria-label={expandedFamilies.has(row.label)
						? `Collapse ${row.label} variants`
						: `Expand ${row.label} variants`}
					onclick={() => toggleFamily(row.label)}
				>
					<span
						class="inline-block text-[0.625rem] transition-transform duration-[var(--duration-fast)]
							{expandedFamilies.has(row.label) ? 'rotate-90' : ''}"
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

		<span class="w-12 shrink-0 text-right text-xs tabular-nums text-text-secondary">
			{isAmbient ? NO_DATA : row.runs}
		</span>
		<span class="w-16 shrink-0 text-right text-xs tabular-nums text-text-secondary">
			{row.kills}
		</span>
		<span class="w-20 shrink-0 text-right text-xs tabular-nums text-text">
			{formatPed(row.cycled)}
		</span>
		<span class="w-20 shrink-0 text-right text-xs tabular-nums font-medium {netTone(net)}">
			{signedPed(net)}
		</span>
	</li>
	{#if isFamily && expandedFamilies.has(row.label)}
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
					{section.isUnassigned ? 'text-text-tertiary' : 'text-text'}"
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
				<span class={COL_RATE} aria-hidden="true"></span>
				<span class={COL_PES} aria-hidden="true"></span>
			{:else}
				<span class="{COL_CYCLED} truncate text-right text-xs tabular-nums text-text">
					{formatPed(section.cycled)}
				</span>
				<span
					class="{COL_RATE} truncate text-right text-xs tabular-nums font-medium {rateTone(
						section.lootRate,
					)}"
				>
					{formatPercent(section.lootRate)}
				</span>
				<span class="{COL_PES} truncate text-right text-xs tabular-nums text-text">
					{section.pesPer100Ped.toFixed(2)}
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
						<Input
							type="search"
							placeholder="Find a session"
							aria-label="Find a session"
							bind:value={query}
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
						onclick={() => onsort('name')}
					>
						Session
						{#if sortKey === 'name'}<span class="text-accent">{sortArrow('name')}</span>{/if}
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
						class="eyebrow {COL_RATE} flex cursor-pointer items-center justify-end gap-1 text-right transition-colors duration-[var(--duration-fast)] hover:text-text"
						aria-label={sortDescription('lootRate', 'TT Rate')}
						onclick={() => onsort('lootRate')}
					>
						TT Rate
						{#if sortKey === 'lootRate'}<span class="text-accent">{sortArrow('lootRate')}</span>{/if}
					</button>
					<button
						type="button"
						class="eyebrow {COL_PES} flex cursor-pointer items-center justify-end gap-1 text-right transition-colors duration-[var(--duration-fast)] hover:text-text"
						aria-label={sortDescription('pesPer100Ped', 'PES per 100 PED')}
						onclick={() => onsort('pesPer100Ped')}
					>
						PES/100
						{#if sortKey === 'pesPer100Ped'}<span class="text-accent"
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
			<div class="min-w-0 p-5">
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
								Their kills and costs still count in Overall and Targets.
							</p>
							<p class="mt-2 text-xs leading-relaxed text-text-tertiary">
								An ended session can be moved into a definition from the dashboard's Review
								surface, after which it counts toward that routine here.
							</p>
						</InfoTip>
					</div>
				{/if}

				<div class="grid grid-cols-3 gap-x-5 gap-y-4">
					<StatDisplay
						label="TT Net"
						value={signedPed(selected.ttNet)}
						valueClass={netTone(selected.ttNet)}
						unit="PED"
					/>
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
							<span class="eyebrow w-12 text-right shrink-0">Runs</span>
							<span class="eyebrow w-16 text-right shrink-0">Kills</span>
							<span class="eyebrow w-20 text-right shrink-0">Cycled</span>
							<span class="eyebrow w-20 text-right shrink-0">TT Net</span>
						</div>
						<ul class="flex max-h-[18rem] flex-col gap-1 overflow-y-auto">
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
						<ul class="flex max-h-[14rem] flex-col gap-1 overflow-y-auto">
							{#each selected.mobs as mob (mob.mobSpecies)}
								<li
									class="flex items-center gap-3 rounded-md px-2.5 py-2 border border-transparent
										hover:bg-surface-hover/30 hover:border-border/40
										transition-[background-color,border-color] duration-[var(--duration-base)] ease-[var(--ease-out)]"
								>
									<span class="flex-1 min-w-0 truncate text-sm font-medium tracking-tight text-text">
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
								Instances
								{#if selected.trend}
									<span class="text-[0.625rem] font-medium uppercase tracking-wide {TREND_TONE[selected.trend]}">
										{TREND_LABEL[selected.trend]}
									</span>
								{/if}
							</span>
							<span class="eyebrow w-14 text-right shrink-0">Time</span>
							<span class="eyebrow w-14 text-right shrink-0">Kills</span>
							<span class="eyebrow w-20 text-right shrink-0">Cycled</span>
							<span class="eyebrow w-20 text-right shrink-0">TT Net</span>
						</div>
						<ul class="flex max-h-[14rem] flex-col gap-1 overflow-y-auto">
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
									<span
										class="w-20 shrink-0 text-right text-sm tabular-nums font-medium {netTone(
											instanceNet,
										)}"
									>
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
