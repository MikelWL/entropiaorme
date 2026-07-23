/**
 * Hunting-tab view model: the per-mob and per-tag comparison data, the
 * archive/main view split with its confirm flow, and the sorted
 * projections. Presentation lives in the tab component; it composes over
 * this state.
 */

import {
	type ArchiveKind,
	activityArchive,
	archive as archiveItem,
	isArchived,
	unarchive as unarchiveItem,
} from '$lib/activityArchive.svelte';
import { getAnalyticsHunting, type HuntingData } from '$lib/api';
import type { MobComparison, TagComparison } from '$lib/types/analytics';
import { describeError } from '$lib/view/errorState';

export type SortDir = 'asc' | 'desc';
export type ViewMode = 'main' | 'archive';

export const ACTION_KEY = '__action';

export const mobColumns = [
	{ key: 'mobName', label: 'Mob', sortable: true, widthClass: 'w-[26%]' },
	{
		key: 'sessions',
		label: 'Sessions',
		align: 'right' as const,
		sortable: true,
		widthClass: 'w-[10%]',
	},
	{ key: 'kills', label: 'Kills', align: 'right' as const, sortable: true, widthClass: 'w-[10%]' },
	{
		key: 'cycled',
		label: 'Cycled',
		align: 'right' as const,
		sortable: true,
		widthClass: 'w-[16%]',
	},
	{
		key: 'pesPer100Ped',
		label: 'PES/100',
		align: 'right' as const,
		sortable: true,
		widthClass: 'w-[16%]',
	},
	{
		key: 'lootRate',
		label: 'Loot',
		align: 'right' as const,
		sortable: true,
		widthClass: 'w-[16%]',
	},
	{ key: ACTION_KEY, label: '', align: 'right' as const, sortable: false, widthClass: 'w-[6%]' },
];

export const tagColumns = [
	{ key: 'tagName', label: 'Tag', sortable: true, widthClass: 'w-[26%]' },
	{
		key: 'sessions',
		label: 'Sessions',
		align: 'right' as const,
		sortable: true,
		widthClass: 'w-[10%]',
	},
	{ key: 'kills', label: 'Kills', align: 'right' as const, sortable: true, widthClass: 'w-[10%]' },
	{
		key: 'cycled',
		label: 'Cycled',
		align: 'right' as const,
		sortable: true,
		widthClass: 'w-[16%]',
	},
	{
		key: 'pesPer100Ped',
		label: 'PES/100',
		align: 'right' as const,
		sortable: true,
		widthClass: 'w-[16%]',
	},
	{
		key: 'lootRate',
		label: 'Loot',
		align: 'right' as const,
		sortable: true,
		widthClass: 'w-[16%]',
	},
	{ key: ACTION_KEY, label: '', align: 'right' as const, sortable: false, widthClass: 'w-[6%]' },
];

export function rowKey(kind: ArchiveKind, name: string): string {
	return `${kind}:${name}`;
}

function sortComparisons<T>(rows: T[], key: keyof T & string, dir: SortDir): T[] {
	return [...rows].sort((a, b) => {
		const aVal = a[key];
		const bVal = b[key];
		if (typeof aVal === 'number' && typeof bVal === 'number') {
			return dir === 'asc' ? aVal - bVal : bVal - aVal;
		}
		return dir === 'asc'
			? String(aVal).localeCompare(String(bVal))
			: String(bVal).localeCompare(String(aVal));
	});
}

export function createHuntingModel() {
	let data = $state<HuntingData | null>(null);
	let loading = $state(true);
	let error = $state<string | null>(null);

	let viewMode = $state<ViewMode>('main');
	let confirmKey = $state<string | null>(null);

	async function loadData() {
		loading = true;
		error = null;
		try {
			data = await getAnalyticsHunting();
		} catch (e) {
			error = describeError(e, 'Failed to load hunting data');
		} finally {
			loading = false;
		}
	}

	async function onArchiveConfirm(kind: ArchiveKind, name: string) {
		error = null;
		try {
			await archiveItem(kind, name);
		} catch (e) {
			error = describeError(e, 'Failed to archive');
		}
		confirmKey = null;
	}

	async function onUnarchiveConfirm(kind: ArchiveKind, name: string) {
		error = null;
		try {
			await unarchiveItem(kind, name);
		} catch (e) {
			error = describeError(e, 'Failed to restore from archive');
		}
		confirmKey = null;
	}

	let mobSortKey = $state<(keyof MobComparison & string) | undefined>('cycled');
	let mobSortDir = $state<SortDir>('desc');

	const sortedMobs = $derived.by(() => {
		if (!data) return [];
		const filtered = data.mobComparisons.filter((m) =>
			viewMode === 'archive'
				? isArchived(activityArchive.current, 'mob', m.mobName)
				: !isArchived(activityArchive.current, 'mob', m.mobName),
		);
		if (!mobSortKey) return filtered;
		return sortComparisons(filtered, mobSortKey, mobSortDir);
	});

	let tagSortKey = $state<(keyof TagComparison & string) | undefined>('cycled');
	let tagSortDir = $state<SortDir>('desc');

	const sortedTags = $derived.by(() => {
		if (!data) return [];
		const filtered = data.tagComparisons.filter((t) =>
			viewMode === 'archive'
				? isArchived(activityArchive.current, 'tag', t.tagName)
				: !isArchived(activityArchive.current, 'tag', t.tagName),
		);
		if (!tagSortKey) return filtered;
		return sortComparisons(filtered, tagSortKey, tagSortDir);
	});

	return {
		get data() {
			return data;
		},
		get loading() {
			return loading;
		},
		get error() {
			return error;
		},
		get viewMode() {
			return viewMode;
		},
		set viewMode(value: ViewMode) {
			viewMode = value;
		},
		get confirmKey() {
			return confirmKey;
		},
		set confirmKey(value: string | null) {
			confirmKey = value;
		},

		get mobSortKey() {
			return mobSortKey;
		},
		set mobSortKey(value: (keyof MobComparison & string) | undefined) {
			mobSortKey = value;
		},
		get mobSortDir() {
			return mobSortDir;
		},
		set mobSortDir(value: SortDir) {
			mobSortDir = value;
		},
		get sortedMobs() {
			return sortedMobs;
		},

		get tagSortKey() {
			return tagSortKey;
		},
		set tagSortKey(value: (keyof TagComparison & string) | undefined) {
			tagSortKey = value;
		},
		get tagSortDir() {
			return tagSortDir;
		},
		set tagSortDir(value: SortDir) {
			tagSortDir = value;
		},
		get sortedTags() {
			return sortedTags;
		},

		loadData,
		onArchiveConfirm,
		onUnarchiveConfirm,
	};
}

export type HuntingModel = ReturnType<typeof createHuntingModel>;
