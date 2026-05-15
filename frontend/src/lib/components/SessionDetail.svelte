<script lang="ts">
	import type { SessionDetail } from '$lib/types/tracking';
	import { formatPed, formatPercent } from '$lib/utils/format';
	import StatDisplay from '$lib/components/StatDisplay.svelte';
	import Badge from '$lib/components/Badge.svelte';
	import Divider from '$lib/components/Divider.svelte';
	import DataTable from '$lib/components/DataTable.svelte';

	let { detail }: { detail: SessionDetail } = $props();

	const weaponCycleRows = $derived.by(() => {
		const rows = detail.toolStats.filter((t) => t.costAttributed > 0);
		const total = rows.reduce((s, t) => s + t.costAttributed, 0);
		return rows
			.map((t) => ({
				weaponName: t.weaponName,
				shotsFired: t.shotsFired,
				costAttributed: t.costAttributed,
				sharePct: total > 0 ? ((t.costAttributed / total) * 100).toFixed(1) : '0.0'
			}))
			.sort((a, b) => b.costAttributed - a.costAttributed);
	});

	const badgeLabels: Record<string, string> = {
		global_kill: 'Global Kill',
		global_item: 'Global Item',
		hof_kill: 'HoF Kill',
		hof_item: 'HoF Item',
		quest_started: 'Quest Started',
		quest_completed: 'Quest Completed',
		quest_completed_pes: 'Quest Completed',
	};

	function badgeVariant(type: SessionDetail['notableEvents'][number]['type']) {
		if (type === 'hof') return 'accent';
		if (type === 'quest') return 'positive';
		return 'warning';
	}
</script>

<div class="bg-surface/50 border border-border/50 rounded-b-md p-5 -mt-1 space-y-5">
	<!-- 1. Summary stats -->
	<div>
		<h3 class="eyebrow mb-3">Summary</h3>
		<div class="grid grid-cols-3 gap-4 sm:grid-cols-6">
			<StatDisplay label="Kills" value={detail.summary.kills} />
			<StatDisplay label="Cycled" value={formatPed(detail.summary.cost)} unit="PED" />
			<StatDisplay label="Loot TT" value={formatPed(detail.summary.returns)} unit="PED" />
			<StatDisplay label="Return" value={formatPercent(detail.summary.returnRate)} />
			<div class="flex flex-col gap-1.5">
				<span class="eyebrow">Net</span>
				<div class="flex items-baseline gap-1.5">
					<span
						class="text-2xl font-semibold tabular-nums leading-none tracking-tight {detail.summary.net >= 0
							? 'text-positive'
							: 'text-negative'}"
					>
						{detail.summary.net >= 0 ? '+' : ''}{formatPed(detail.summary.net)}
					</span>
					<span class="text-xs font-medium text-text-tertiary uppercase tracking-wider">PED</span>
				</div>
			</div>
			<StatDisplay label="PES" value={formatPed(detail.summary.pes ?? 0)} unit="PES" />
		</div>
	</div>

	<!-- 1b. Cost breakdown (shown when non-weapon costs exist) -->
	{#if detail.summary.costBreakdown && (detail.summary.costBreakdown.healCost > 0 || detail.summary.costBreakdown.enhancerCost > 0 || detail.summary.costBreakdown.armourCost > 0)}
		<div class="mt-2 pl-1 flex flex-wrap gap-x-5 gap-y-1 text-xs text-text-secondary">
			<span>Weapon: <span class="text-text tabular-nums">{formatPed(detail.summary.costBreakdown.weaponCost)}</span></span>
			{#if detail.summary.costBreakdown.healCost > 0}
				<span>Healing: <span class="text-text tabular-nums">{formatPed(detail.summary.costBreakdown.healCost)}</span></span>
			{/if}
			{#if detail.summary.costBreakdown.enhancerCost > 0}
				<span>Enhancers: <span class="text-text tabular-nums">{formatPed(detail.summary.costBreakdown.enhancerCost)}</span></span>
			{/if}
			{#if detail.summary.costBreakdown.armourCost > 0}
				<span>Armour: <span class="text-text tabular-nums">{formatPed(detail.summary.costBreakdown.armourCost)}</span></span>
			{/if}
		</div>
	{/if}

	<!-- 1c. Weapon cycle breakdown -->
	{#if weaponCycleRows.length > 0}
		<Divider />
		<div>
			<h3 class="eyebrow mb-3">
				Weapon Cycle
			</h3>
			<DataTable
				columns={[
					{ key: 'weaponName', label: 'Weapon' },
					{ key: 'shotsFired', label: 'Shots', align: 'right' },
					{ key: 'costAttributed', label: 'Cycle', align: 'right' },
					{ key: 'sharePct', label: 'Share', align: 'right' }
				]}
				rows={weaponCycleRows}
			>
				{#snippet cell({ row, column, value })}
					{#if column.key === 'costAttributed'}
						{formatPed(row.costAttributed)}
					{:else if column.key === 'sharePct'}
						{row.sharePct}%
					{:else}
						{value}
					{/if}
				{/snippet}
			</DataTable>
		</div>
	{/if}

	<!-- 2. Notable events -->
	{#if detail.notableEvents.length > 0}
		<Divider />
		<div>
			<h3 class="eyebrow mb-3">
				Notable Events
			</h3>
			<div class="space-y-2">
				{#each detail.notableEvents as event}
					<div
						class="flex items-center justify-between bg-surface-hover/50 rounded-md px-3 py-2"
					>
						<div class="flex items-center gap-2">
							<Badge variant={badgeVariant(event.type)}>
								{badgeLabels[event.eventType] ?? 'Event'}
							</Badge>
							<span class="text-sm text-text">{event.target}</span>
							{#if event.item && event.item !== event.target}
								<span class="text-xs text-text-tertiary">&mdash;</span>
								<span class="text-sm text-text-secondary">{event.item}</span>
							{/if}
						</div>
						<span class="text-sm font-medium text-positive tabular-nums">
							{formatPed(event.value)} {event.eventType === 'quest_completed_pes' ? 'PES' : 'PED'}
						</span>
					</div>
				{/each}
			</div>
		</div>
	{/if}

	<!-- 3. Loot breakdown -->
	<Divider />
	<div>
		<h3 class="eyebrow mb-3">
			Loot Breakdown
		</h3>
		<DataTable
			columns={[
				{ key: 'name', label: 'Item' },
				{ key: 'quantity', label: 'Qty', align: 'right' },
				{ key: 'ttValue', label: 'TT Value', align: 'right' }
			]}
			rows={detail.lootBreakdown}
		>
			{#snippet cell({ row, column, value })}
				{#if column.key === 'ttValue'}
					{formatPed(row.ttValue)}
				{:else}
					{value}
				{/if}
			{/snippet}
		</DataTable>
	</div>

	<!-- 4. Skill gains -->
	{#if detail.skillGains.length > 0}
		<Divider />
		<div>
			<h3 class="eyebrow mb-3">
				Skill Gains
			</h3>
			<div class="space-y-1.5">
				{#each detail.skillGains as skill}
					<div class="flex items-center justify-between text-sm">
						<div class="flex items-center gap-2">
							<span class="text-text">{skill.skillName}</span>
							<span class="text-xs text-text-tertiary">Lv {skill.level}</span>
						</div>
						<span class="text-positive tabular-nums font-medium">
							+{formatPed(skill.ttValueGained)} PES
						</span>
					</div>
				{/each}
			</div>
		</div>
	{/if}
</div>
