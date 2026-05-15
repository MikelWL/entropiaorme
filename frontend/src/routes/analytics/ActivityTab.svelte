<script lang="ts">
	import type { MobComparison, TagComparison, WeaponComparison } from '$lib/types/analytics';
	import { getAnalyticsActivity, type ActivityData } from '$lib/api';
	import { formatPed, formatPercent } from '$lib/utils/format';
	import DataTable from '$lib/components/DataTable.svelte';
	import Divider from '$lib/components/Divider.svelte';
	import Card from '$lib/components/Card.svelte';
	import {
		activityArchive,
		archive as archiveItem,
		unarchive as unarchiveItem,
		isArchived,
		type ArchiveKind,
	} from '$lib/activityArchive';

	let data = $state<ActivityData | null>(null);
	let loading = $state(true);
	let error = $state<string | null>(null);

	$effect(() => {
		loadData();
	});

	async function loadData() {
		loading = true;
		error = null;
		try {
			data = await getAnalyticsActivity();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load activity data';
		} finally {
			loading = false;
		}
	}

	let viewMode = $state<'main' | 'archive'>('main');
	let confirmKey = $state<string | null>(null);

	function rowKey(kind: ArchiveKind, name: string): string {
		return `${kind}:${name}`;
	}

	async function onArchiveConfirm(kind: ArchiveKind, name: string) {
		await archiveItem(kind, name);
		confirmKey = null;
	}

	async function onUnarchiveConfirm(kind: ArchiveKind, name: string) {
		await unarchiveItem(kind, name);
		confirmKey = null;
	}

	let mobSortKey = $state<(keyof MobComparison & string) | undefined>('cycled');
	let mobSortDir = $state<'asc' | 'desc'>('desc');

	let sortedMobs = $derived.by(() => {
		if (!data) return [];
		const archived = $activityArchive;
		const filtered = data.mobComparisons.filter((m) =>
			viewMode === 'archive'
				? isArchived(archived, 'mob', m.mobName)
				: !isArchived(archived, 'mob', m.mobName)
		);
		if (!mobSortKey) return filtered;
		const key = mobSortKey;
		return [...filtered].sort((a, b) => {
			const aVal = a[key];
			const bVal = b[key];
			if (typeof aVal === 'number' && typeof bVal === 'number') {
				return mobSortDir === 'asc' ? aVal - bVal : bVal - aVal;
			}
			return mobSortDir === 'asc'
				? String(aVal).localeCompare(String(bVal))
				: String(bVal).localeCompare(String(aVal));
		});
	});

	let tagSortKey = $state<(keyof TagComparison & string) | undefined>('cycled');
	let tagSortDir = $state<'asc' | 'desc'>('desc');

	let sortedTags = $derived.by(() => {
		if (!data) return [];
		const archived = $activityArchive;
		const filtered = data.tagComparisons.filter((t) =>
			viewMode === 'archive'
				? isArchived(archived, 'tag', t.tagName)
				: !isArchived(archived, 'tag', t.tagName)
		);
		if (!tagSortKey) return filtered;
		const key = tagSortKey;
		return [...filtered].sort((a, b) => {
			const aVal = a[key];
			const bVal = b[key];
			if (typeof aVal === 'number' && typeof bVal === 'number') {
				return tagSortDir === 'asc' ? aVal - bVal : bVal - aVal;
			}
			return tagSortDir === 'asc'
				? String(aVal).localeCompare(String(bVal))
				: String(bVal).localeCompare(String(aVal));
		});
	});

	let weaponSortKey = $state<(keyof WeaponComparison & string) | undefined>('cycled');
	let weaponSortDir = $state<'asc' | 'desc'>('desc');

	let sortedWeapons = $derived.by(() => {
		if (!data) return [];
		const archived = $activityArchive;
		const filtered = data.weaponComparisons.filter((w) =>
			viewMode === 'archive'
				? isArchived(archived, 'weapon', w.weaponName)
				: !isArchived(archived, 'weapon', w.weaponName)
		);
		if (!weaponSortKey) return filtered;
		const key = weaponSortKey;
		return [...filtered].sort((a, b) => {
			const aVal = a[key];
			const bVal = b[key];
			if (typeof aVal === 'number' && typeof bVal === 'number') {
				return weaponSortDir === 'asc' ? aVal - bVal : bVal - aVal;
			}
			return weaponSortDir === 'asc'
				? String(aVal).localeCompare(String(bVal))
				: String(bVal).localeCompare(String(aVal));
		});
	});

	const ACTION_KEY = '__action';

	const mobColumns = [
		{ key: 'mobName', label: 'Mob', sortable: true, widthClass: 'w-[26%]' },
		{ key: 'sessions', label: 'Sessions', align: 'right' as const, sortable: true, widthClass: 'w-[10%]' },
		{ key: 'kills', label: 'Kills', align: 'right' as const, sortable: true, widthClass: 'w-[10%]' },
		{ key: 'cycled', label: 'Cycled', align: 'right' as const, sortable: true, widthClass: 'w-[16%]' },
		{ key: 'pesPer100Ped', label: 'PES/100', align: 'right' as const, sortable: true, widthClass: 'w-[16%]' },
		{ key: 'lootRate', label: 'Loot', align: 'right' as const, sortable: true, widthClass: 'w-[16%]' },
		{ key: ACTION_KEY, label: '', align: 'right' as const, sortable: false, widthClass: 'w-[6%]' },
	];

	const tagColumns = [
		{ key: 'tagName', label: 'Tag', sortable: true, widthClass: 'w-[26%]' },
		{ key: 'sessions', label: 'Sessions', align: 'right' as const, sortable: true, widthClass: 'w-[10%]' },
		{ key: 'kills', label: 'Kills', align: 'right' as const, sortable: true, widthClass: 'w-[10%]' },
		{ key: 'cycled', label: 'Cycled', align: 'right' as const, sortable: true, widthClass: 'w-[16%]' },
		{ key: 'pesPer100Ped', label: 'PES/100', align: 'right' as const, sortable: true, widthClass: 'w-[16%]' },
		{ key: 'lootRate', label: 'Loot', align: 'right' as const, sortable: true, widthClass: 'w-[16%]' },
		{ key: ACTION_KEY, label: '', align: 'right' as const, sortable: false, widthClass: 'w-[6%]' },
	];

	const weaponColumns = [
		{ key: 'weaponName', label: 'Weapon', sortable: true, widthClass: 'w-[26%]' },
		{ key: 'sessions', label: 'Sessions', align: 'right' as const, sortable: true, widthClass: 'w-[10%]' },
		{ key: 'kills', label: 'Kills', align: 'right' as const, sortable: true, widthClass: 'w-[10%]' },
		{ key: 'cycled', label: 'Cycled', align: 'right' as const, sortable: true, widthClass: 'w-[16%]' },
		{ key: 'pesPer100Ped', label: 'PES/100', align: 'right' as const, sortable: true, widthClass: 'w-[16%]' },
		{ key: 'lootRate', label: 'Loot', align: 'right' as const, sortable: true, widthClass: 'w-[16%]' },
		{ key: ACTION_KEY, label: '', align: 'right' as const, sortable: false, widthClass: 'w-[6%]' },
	];
</script>

{#snippet archiveAction(kind: ArchiveKind, name: string)}
	{#if viewMode === 'main'}
		<button
			type="button"
			class="text-text-tertiary hover:text-text transition-colors duration-[var(--duration-fast)] cursor-pointer p-1"
			onclick={(e) => { e.stopPropagation(); confirmKey = rowKey(kind, name); }}
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
			onclick={(e) => { e.stopPropagation(); confirmKey = rowKey(kind, name); }}
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
	{@const isRestore = viewMode === 'archive'}
	<div class="inline-flex items-center gap-3">
		<span class="text-xs text-text-secondary">
			{isRestore ? 'Send back to main activity records?' : 'Send record to archive?'}
		</span>
		<button
			type="button"
			class="text-xs text-text-secondary hover:text-text px-2 py-0.5 rounded-sm cursor-pointer border border-border/60 hover:border-border-bright"
			onclick={(e) => { e.stopPropagation(); confirmKey = null; }}
		>
			Cancel
		</button>
		<button
			type="button"
			class="text-xs text-accent hover:text-accent-hover px-2 py-0.5 rounded-sm cursor-pointer border border-accent/40 hover:border-accent font-medium"
			onclick={(e) => {
				e.stopPropagation();
				if (isRestore) onUnarchiveConfirm(kind, name);
				else onArchiveConfirm(kind, name);
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

{#snippet tagCell({ column, value, row }: { column: { key: string }; value: unknown; row: TagComparison })}
	{#if column.key === 'cycled'}
		<span class="tabular-nums">{formatPed(Number(value))}</span>
	{:else if column.key === 'pesPer100Ped'}
		<span class="tabular-nums">{Number(value).toFixed(2)}</span>
	{:else if column.key === 'lootRate'}
		<span class="tabular-nums">{formatPercent(Number(value))}</span>
	{:else if column.key === ACTION_KEY}
		{@render archiveAction('tag', row.tagName)}
	{:else}
		{value}
	{/if}
{/snippet}

{#snippet weaponCell({ column, value, row }: { column: { key: string }; value: unknown; row: WeaponComparison })}
	{#if column.key === 'cycled'}
		<span class="tabular-nums">{formatPed(Number(value))}</span>
	{:else if column.key === 'pesPer100Ped'}
		<span class="tabular-nums">{Number(value).toFixed(2)}</span>
	{:else if column.key === 'lootRate'}
		<span class="tabular-nums">{formatPercent(Number(value))}</span>
	{:else if column.key === ACTION_KEY}
		{@render archiveAction('weapon', row.weaponName)}
	{:else}
		{value}
	{/if}
{/snippet}

{#if loading}
	<p class="text-sm text-text-secondary">Loading activity data...</p>
{:else if error}
	<p class="text-sm text-error">{error}</p>
{:else if data}
	<div class="space-y-6" data-guide-anchor="analytics-activity-area">
		{#if viewMode === 'archive'}
			<div class="flex items-center justify-between">
				<h3 class="text-sm font-medium text-text-secondary">Archived rows</h3>
				<button
					type="button"
					class="text-sm text-text-secondary hover:text-text transition-colors duration-[var(--duration-fast)] cursor-pointer inline-flex items-center gap-1"
					onclick={() => { viewMode = 'main'; confirmKey = null; }}
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
		{#snippet tagOverlay({ row }: { row: TagComparison })}
			{@render confirmPrompt('tag', row.tagName)}
		{/snippet}
		{#snippet weaponOverlay({ row }: { row: WeaponComparison })}
			{@render confirmPrompt('weapon', row.weaponName)}
		{/snippet}

		<!-- Per-mob comparison -->
		<div>
			<h3 class="eyebrow mb-3">Per-Mob Comparison</h3>
			<DataTable
				columns={mobColumns}
				rows={sortedMobs}
				bind:sortKey={mobSortKey}
				bind:sortDir={mobSortDir}
				cell={mobCell}
				fixedLayout={true}
				rowKeyFn={(r: MobComparison) => rowKey('mob', r.mobName)}
				overlayKey={confirmKey}
				rowOverlay={mobOverlay}
				emptyMessage={viewMode === 'archive' ? 'No archived mobs' : 'No mob data available'}
			/>
		</div>

		<Divider />

		<div>
			<h3 class="eyebrow mb-3">Per-Tag Comparison</h3>
			<DataTable
				columns={tagColumns}
				rows={sortedTags}
				bind:sortKey={tagSortKey}
				bind:sortDir={tagSortDir}
				cell={tagCell}
				fixedLayout={true}
				rowKeyFn={(r: TagComparison) => rowKey('tag', r.tagName)}
				overlayKey={confirmKey}
				rowOverlay={tagOverlay}
				emptyMessage={viewMode === 'archive' ? 'No archived tags' : 'No tagged hunt data available'}
			/>
		</div>

		<Divider />

		<!-- Per-weapon comparison -->
		<div>
			<h3 class="eyebrow mb-3">Per-Weapon Comparison</h3>
			<DataTable
				columns={weaponColumns}
				rows={sortedWeapons}
				bind:sortKey={weaponSortKey}
				bind:sortDir={weaponSortDir}
				cell={weaponCell}
				fixedLayout={true}
				rowKeyFn={(r: WeaponComparison) => rowKey('weapon', r.weaponName)}
				overlayKey={confirmKey}
				rowOverlay={weaponOverlay}
				emptyMessage={viewMode === 'archive' ? 'No archived weapons' : 'No weapon data available'}
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
			{#if viewMode === 'main'}
				<button
					type="button"
					class="text-sm text-text-secondary hover:text-text transition-colors duration-[var(--duration-fast)] cursor-pointer inline-flex items-center gap-1.5 shrink-0"
					onclick={() => { viewMode = 'archive'; confirmKey = null; }}
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
			No tracking data yet. Complete sessions to see activity comparisons.
		</p>
	</Card>
{/if}
