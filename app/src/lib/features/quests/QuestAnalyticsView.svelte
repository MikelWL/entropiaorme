<script lang="ts">
	import { Card, DataTable, ErrorNotice, SegmentedControl } from '$lib/components';
	import { formatPed, formatPercent } from '$lib/utils/format';
	import { createTableModel } from '$lib/view/tableModel.svelte';
	import {
		computePlaylistAnalytics,
		computeQuestAnalytics,
		type PlaylistAnalyticsComputed,
		type QuestAnalyticsComputed,
		type RewardMode
	} from './economics';
	import type { QuestsModel } from './questsModel.svelte';

	let { model }: { model: QuestsModel } = $props();

	const computedAnalytics = $derived(
		computeQuestAnalytics(model.analyticsData, model.rates, model.analyticsRewardMode)
	);
	const computedPlaylistAnalytics = $derived(
		computePlaylistAnalytics(model.playlistAnalyticsData, model.rates, model.analyticsRewardMode)
	);

	// The default table-model comparator sorts nulls last in both directions;
	// this column historically fell back to String() comparison when either
	// side is null, so that ordering is pinned here.
	function markupCompare(a: QuestAnalyticsComputed, b: QuestAnalyticsComputed): number {
		const aVal = a.rewardMarkupPercent;
		const bVal = b.rewardMarkupPercent;
		if (typeof aVal === 'number' && typeof bVal === 'number') return aVal - bVal;
		return String(aVal).localeCompare(String(bVal));
	}

	// Sort-only adoption: the analytics table renders all rows, so the page
	// size just has to keep everything on page one.
	const table = createTableModel<QuestAnalyticsComputed>({
		rows: () => computedAnalytics,
		pageSize: Number.MAX_SAFE_INTEGER,
		comparators: { rewardMarkupPercent: markupCompare }
	});

	type ColumnDef<T> = {
		key: keyof T & string;
		label: string;
		align?: 'left' | 'right' | 'center';
		sortable?: boolean;
	};

	const analyticsColumns = $derived.by((): ColumnDef<QuestAnalyticsComputed>[] => {
		const columns: ColumnDef<QuestAnalyticsComputed>[] = [
			{ key: 'questName', label: 'Quest', sortable: true },
			{ key: 'linkedSessions', label: 'Sessions', align: 'right', sortable: true },
			{ key: 'displayLiquidReward', label: 'Reward', align: 'right', sortable: true },
			{ key: 'avgCycled', label: 'Avg Cycled', align: 'right', sortable: true }
		];
		if (model.analyticsRewardMode === 'markup') {
			columns.push({ key: 'rewardMarkupPercent', label: 'Markup', align: 'right', sortable: true });
		}
		columns.push(
			{ key: 'avgNet', label: 'Avg Net', align: 'right', sortable: true },
			{ key: 'returnRate', label: 'Rate', align: 'right', sortable: true }
		);
		return columns;
	});

	const playlistAnalyticsColumns = $derived.by((): ColumnDef<PlaylistAnalyticsComputed>[] => {
		const columns: ColumnDef<PlaylistAnalyticsComputed>[] = [
			{ key: 'playlistName', label: 'Playlist', sortable: true },
			{ key: 'displayImmediateReward', label: 'Base Reward', align: 'right', sortable: true },
			{ key: 'displayBonusReward', label: 'Bonus/Run', align: 'right', sortable: true },
			{ key: 'avgCycled', label: 'Avg Cycled', align: 'right', sortable: true }
		];
		if (model.analyticsRewardMode === 'markup') {
			columns.push({ key: 'rewardMarkupPercent', label: 'Markup', align: 'right', sortable: true });
		}
		columns.push(
			{ key: 'avgNet', label: 'Avg Net', align: 'right', sortable: true },
			{ key: 'returnRate', label: 'Rate', align: 'right', sortable: true }
		);
		return columns;
	});
</script>

{#if model.analyticsLoading}
	<div class="text-sm text-text-tertiary py-8 text-center">Loading quest analytics...</div>
{:else if model.analyticsError}
	<ErrorNotice message={model.analyticsError} />
{:else if computedAnalytics.length === 0}
	<Card class="p-6">
		<p class="text-sm text-text-tertiary text-center">
			No curated quest analytics yet. Quest tracking continues in the background, but analytics only include sessions you explicitly link after a clean tracked run.
		</p>
	</Card>
{:else}
	<div class="space-y-3">
		<div class="flex flex-wrap items-center justify-between gap-2">
			<h3 class="text-sm font-medium text-text-secondary">Single Quest Analytics</h3>
			<SegmentedControl
				options={[
					{ id: 'tt', label: 'TT Only' },
					{ id: 'markup', label: 'With Reward Markup' }
				]}
				active={model.analyticsRewardMode}
				onchange={(id) => (model.analyticsRewardMode = id as RewardMode)}
			/>
		</div>
		{#snippet analyticsCell({ column, value, row }: { column: { key: string }; value: unknown; row: QuestAnalyticsComputed })}
			{#if column.key === 'questName'}
				<span class="font-medium">{value}</span>
			{:else if column.key === 'displayLiquidReward'}
				<div class="flex flex-col items-end leading-tight">
					<span class="tabular-nums">{formatPed(Number(value))}</span>
					{#if row.avgRewardPes > 0}
						<span class="text-[11px] text-accent">+{formatPed(row.avgRewardPes)} PES</span>
					{/if}
				</div>
			{:else if column.key === 'avgCycled'}
				<span class="tabular-nums">{formatPed(Number(value))}</span>
			{:else if column.key === 'avgNet'}
				<div class="flex flex-col items-end leading-tight">
					<span class="tabular-nums {Number(value) >= 0 ? 'text-positive' : 'text-negative'}">
						{Number(value) >= 0 ? '+' : ''}{formatPed(Number(value))}
					</span>
					{#if row.avgPesNet > 0}
						<span class="text-[11px] text-accent">+{formatPed(row.avgPesNet)} PES</span>
					{/if}
				</div>
			{:else if column.key === 'rewardMarkupPercent'}
				<span class="tabular-nums text-text-secondary">
					{value == null ? '\u2014' : `${Number(value).toFixed(0)}%`}
				</span>
			{:else if column.key === 'returnRate'}
				<span class="tabular-nums">{formatPercent(Number(value))}</span>
			{:else}
				{value}
			{/if}
		{/snippet}
		<DataTable
			columns={analyticsColumns}
			rows={table.filtered}
			bind:sortKey={() => table.sortKey, (key) => {
				if (key !== undefined && key !== table.sortKey) table.setSort(key);
			}}
			bind:sortDir={() => table.sortDir, (dir) => {
				if (table.sortKey !== undefined && dir !== table.sortDir) table.setSort(table.sortKey);
			}}
			cell={analyticsCell}
			emptyMessage="No curated quest runs"
		/>

		<!-- Playlist Analytics -->
		{#if computedPlaylistAnalytics.length > 0}
			<h3 class="text-sm font-medium text-text-secondary mt-6 mb-2">Playlist Analytics</h3>
			{#snippet playlistCell({ column, value, row }: { column: { key: string }; value: unknown; row: PlaylistAnalyticsComputed })}
				{#if column.key === 'playlistName'}
					<span class="font-medium">{value}</span>
				{:else if column.key === 'displayImmediateReward' || column.key === 'displayBonusReward'}
					{@const pesPortion = column.key === 'displayImmediateReward'
						? row.avgImmediateSkillReward
						: row.avgBonusSkillReward}
					<div class="flex flex-col items-end leading-tight">
						<span class="tabular-nums">{formatPed(Number(value))}</span>
						{#if pesPortion > 0}
							<span class="text-[11px] text-accent">+{formatPed(pesPortion)} PES</span>
						{/if}
					</div>
				{:else if column.key === 'avgCycled'}
					<span class="tabular-nums">{formatPed(Number(value))}</span>
				{:else if column.key === 'avgNet'}
					<div class="flex flex-col items-end leading-tight">
						<span class="tabular-nums {Number(value) >= 0 ? 'text-positive' : 'text-negative'}">
							{Number(value) >= 0 ? '+' : ''}{formatPed(Number(value))}
						</span>
						{#if row.avgPesNet > 0}
							<span class="text-[11px] text-accent">+{formatPed(row.avgPesNet)} PES</span>
						{/if}
					</div>
				{:else if column.key === 'rewardMarkupPercent'}
					<span class="tabular-nums text-text-secondary">
						{value == null ? '\u2014' : `${Number(value).toFixed(0)}%`}
					</span>
				{:else if column.key === 'returnRate'}
					<span class="tabular-nums">{formatPercent(Number(value))}</span>
				{:else}
					{value}
				{/if}
			{/snippet}
			<DataTable
				columns={playlistAnalyticsColumns}
				rows={computedPlaylistAnalytics}
				cell={playlistCell}
				emptyMessage="No curated playlist runs"
			/>
		{/if}

		<div class="text-[11px] text-text-tertiary tabular-nums pt-2 text-right">
			Liquid baseline: {formatPercent(model.rates.liquidReturnRate)}
		</div>
	</div>
{/if}
