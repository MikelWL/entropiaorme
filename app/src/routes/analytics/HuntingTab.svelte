<script lang="ts">
	import type { ArchiveKind } from '$lib/activityArchive.svelte';
	import Card from '$lib/components/Card.svelte';
	import DataTable from '$lib/components/DataTable.svelte';
	import Divider from '$lib/components/Divider.svelte';
	import ErrorNotice from '$lib/components/ErrorNotice.svelte';
	import {
		ACTION_KEY,
		createHuntingModel,
		mobColumns,
		rowKey,
		nameColumns
	} from '$lib/features/analytics/huntingModel.svelte';
	import type { MobComparison, NameComparison } from '$lib/types/analytics';
	import { formatPed, formatPercent } from '$lib/utils/format';

	const model = createHuntingModel();

	$effect(() => {
		void model.loadData();
	});
</script>

{#snippet archiveAction(kind: ArchiveKind, name: string)}
	{#if model.viewMode === 'main'}
		<button
			type="button"
			class="text-text-tertiary hover:text-text transition-colors duration-[var(--duration-fast)] cursor-pointer p-1"
			onclick={(e) => { e.stopPropagation(); model.confirmKey = rowKey(kind, name); }}
			aria-label="Archive {name}"
			title="Archive"
		>
			<svg
				xmlns="http://www.w3.org/2000/svg"
				fill="none"
				viewBox="0 0 24 24"
				stroke-width="1.5"
				stroke="currentColor"
				class="w-4 h-4"
			>
				<path
					stroke-linecap="round"
					stroke-linejoin="round"
					d="M20.25 7.5l-.625 10.632a2.25 2.25 0 0 1-2.247 2.118H6.622a2.25 2.25 0 0 1-2.247-2.118L3.75 7.5M10 11.25h4M3.375 7.5h17.25c.621 0 1.125-.504 1.125-1.125v-1.5c0-.621-.504-1.125-1.125-1.125H3.375c-.621 0-1.125.504-1.125 1.125v1.5c0 .621.504 1.125 1.125 1.125z"
				/>
			</svg>
		</button>
	{:else}
		<button
			type="button"
			class="text-text-tertiary hover:text-text transition-colors duration-[var(--duration-fast)] cursor-pointer p-1"
			onclick={(e) => { e.stopPropagation(); model.confirmKey = rowKey(kind, name); }}
			aria-label="Restore {name}"
			title="Restore from archive"
		>
			<svg
				xmlns="http://www.w3.org/2000/svg"
				fill="none"
				viewBox="0 0 24 24"
				stroke-width="1.5"
				stroke="currentColor"
				class="w-4 h-4"
			>
				<path
					stroke-linecap="round"
					stroke-linejoin="round"
					d="M20.25 7.5l-.625 10.632a2.25 2.25 0 0 1-2.247 2.118H6.622a2.25 2.25 0 0 1-2.247-2.118L3.75 7.5m8.25 3.75l2.25 2.25m0-2.25l-2.25 2.25M3.375 7.5h17.25c.621 0 1.125-.504 1.125-1.125v-1.5c0-.621-.504-1.125-1.125-1.125H3.375c-.621 0-1.125.504-1.125 1.125v1.5c0 .621.504 1.125 1.125 1.125z"
				/>
			</svg>
		</button>
	{/if}
{/snippet}

{#snippet confirmPrompt(kind: ArchiveKind, name: string)}
	{@const isRestore = model.viewMode === 'archive'}
	<div class="inline-flex items-center gap-3">
		<span class="text-xs text-text-secondary">
			{isRestore ? 'Send back to main hunting records?' : 'Send record to archive?'}
		</span>
		<button
			type="button"
			class="text-xs text-text-secondary hover:text-text px-2 py-0.5 rounded-sm cursor-pointer border border-border/60 hover:border-border-bright"
			onclick={(e) => { e.stopPropagation(); model.confirmKey = null; }}
		>
			Cancel
		</button>
		<button
			type="button"
			class="text-xs text-accent hover:text-accent-hover px-2 py-0.5 rounded-sm cursor-pointer border border-accent/40 hover:border-accent font-medium"
			onclick={(e) => {
				e.stopPropagation();
				if (isRestore) model.onUnarchiveConfirm(kind, name);
				else model.onArchiveConfirm(kind, name);
			}}
		>
			Yes
		</button>
	</div>
{/snippet}

{#snippet mobCell({ column, value, row }: { column: { key: string }; value: unknown; row: MobComparison })}
	{#if column.key === 'cycled'}
		<span class="tabular-nums">{formatPed(Number(value))}</span>
	{:else if column.key === 'pesPer100Ped'}
		<span class="tabular-nums">{Number(value).toFixed(2)}</span>
	{:else if column.key === 'lootRate'}
		<span class="tabular-nums">{formatPercent(Number(value))}</span>
	{:else if column.key === ACTION_KEY}
		{@render archiveAction('mob', row.mobName)}
	{:else}
		{value}
	{/if}
{/snippet}

{#snippet nameCell({ column, value, row }: { column: { key: string }; value: unknown; row: NameComparison })}
	{#if column.key === 'cycled'}
		<span class="tabular-nums">{formatPed(Number(value))}</span>
	{:else if column.key === 'pesPer100Ped'}
		<span class="tabular-nums">{Number(value).toFixed(2)}</span>
	{:else if column.key === 'lootRate'}
		<span class="tabular-nums">{formatPercent(Number(value))}</span>
	{:else if column.key === ACTION_KEY}
		{@render archiveAction('name', row.sessionName)}
	{:else}
		{value}
	{/if}
{/snippet}

{#if model.loading}
	<p class="text-sm text-text-secondary">Loading hunting data...</p>
{:else if model.error && !model.data}
	<ErrorNotice message={model.error} />
{:else if model.data}
	<div class="space-y-6" data-guide-anchor="analytics-hunting-area">
		<ErrorNotice message={model.error} />
		{#if model.viewMode === 'archive'}
			<div class="flex items-center justify-between">
				<h3 class="text-sm font-medium text-text-secondary">Archived rows</h3>
				<button
					type="button"
					class="text-sm text-text-secondary hover:text-text transition-colors duration-[var(--duration-fast)] cursor-pointer inline-flex items-center gap-1"
					onclick={() => { model.viewMode = 'main'; model.confirmKey = null; }}
				>
					<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="w-3.5 h-3.5">
						<path fill-rule="evenodd" d="M9.78 4.22a.75.75 0 010 1.06L7.06 8H15a.75.75 0 010 1.5H7.06l2.72 2.72a.75.75 0 11-1.06 1.06l-4-4a.75.75 0 010-1.06l4-4a.75.75 0 011.06 0z" clip-rule="evenodd" transform="translate(0 2)" />
					</svg>
					Back to activity
				</button>
			</div>
		{/if}

		{#snippet mobOverlay({ row }: { row: MobComparison })}
			{@render confirmPrompt('mob', row.mobName)}
		{/snippet}
		{#snippet nameOverlay({ row }: { row: NameComparison })}
			{@render confirmPrompt('name', row.sessionName)}
		{/snippet}
		<!-- Per-mob comparison -->
		<div>
			<h3 class="eyebrow mb-3">Per-Mob Comparison</h3>
			<DataTable
				columns={mobColumns}
				rows={model.sortedMobs}
				bind:sortKey={model.mobSortKey}
				bind:sortDir={model.mobSortDir}
				cell={mobCell}
				fixedLayout={true}
				rowKeyFn={(r: MobComparison) => rowKey('mob', r.mobName)}
				overlayKey={model.confirmKey}
				rowOverlay={mobOverlay}
				emptyMessage={model.viewMode === 'archive' ? 'No archived mobs' : 'No mob data available'}
			/>
		</div>

		<Divider />

		<div>
			<h3 class="eyebrow mb-3">Per-Session Comparison</h3>
			<DataTable
				columns={nameColumns}
				rows={model.sortedNames}
				bind:sortKey={model.nameSortKey}
				bind:sortDir={model.nameSortDir}
				cell={nameCell}
				fixedLayout={true}
				rowKeyFn={(r: NameComparison) => rowKey('name', r.sessionName)}
				overlayKey={model.confirmKey}
				rowOverlay={nameOverlay}
				emptyMessage={model.viewMode === 'archive' ? 'No archived sessions' : 'No named hunt sessions yet'}
			/>
		</div>

		<Divider />

		<div class="flex items-end justify-between gap-6">
			<div class="space-y-1 text-xs text-text-tertiary flex-1 min-w-0">
				<p>
					<span class="text-text-secondary">PES:</span>
					Project Entropia Skill: non-liquid skill-progress denomination derived from the skill curve.
				</p>
				<p>
					<span class="text-text-secondary">PES/100:</span>
					PES per 100 PED cycled; the primary skilling comparison.
				</p>
				<p>
					<span class="text-text-secondary">Loot:</span>
					loot-only return per cycled PED; useful, but more volatile.
				</p>
			</div>
			{#if model.viewMode === 'main'}
				<button
					type="button"
					class="text-sm text-text-secondary hover:text-text transition-colors duration-[var(--duration-fast)] cursor-pointer inline-flex items-center gap-1.5 shrink-0"
					onclick={() => { model.viewMode = 'archive'; model.confirmKey = null; }}
				>
					<svg
						xmlns="http://www.w3.org/2000/svg"
						fill="none"
						viewBox="0 0 24 24"
						stroke-width="1.5"
						stroke="currentColor"
						class="w-4 h-4"
					>
						<path
							stroke-linecap="round"
							stroke-linejoin="round"
							d="M20.25 7.5l-.625 10.632a2.25 2.25 0 0 1-2.247 2.118H6.622a2.25 2.25 0 0 1-2.247-2.118L3.75 7.5M10 11.25h4M3.375 7.5h17.25c.621 0 1.125-.504 1.125-1.125v-1.5c0-.621-.504-1.125-1.125-1.125H3.375c-.621 0-1.125.504-1.125 1.125v1.5c0 .621.504 1.125 1.125 1.125z"
						/>
					</svg>
					View archive
				</button>
			{/if}
		</div>
	</div>
{:else}
	<Card class="p-6">
		<p class="text-sm text-text-tertiary text-center">
			No tracking data yet. Complete sessions to see hunting comparisons.
		</p>
	</Card>
{/if}
