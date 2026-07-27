import { getPreference, setPreference } from './preferences';

export type ArchiveKind = 'mob' | 'name';

export type ActivityArchiveState = {
	mobs: string[];
	names: string[];
};

const KEY = 'activityArchive';

const EMPTY: ActivityArchiveState = { mobs: [], names: [] };

let archiveState = $state<ActivityArchiveState>(EMPTY);

// Direct writes are for test arrangement; app code mutates through
// archive/unarchive so every change persists.
export const activityArchive = {
	get current(): ActivityArchiveState {
		return archiveState;
	},
	set current(value: ActivityArchiveState) {
		archiveState = value;
	},
};

function sanitise(value: unknown): ActivityArchiveState {
	const v = (value ?? {}) as Partial<ActivityArchiveState>;
	const arr = (x: unknown): string[] =>
		Array.isArray(x)
			? Array.from(new Set(x.filter((s): s is string => typeof s === 'string')))
			: [];
	// A stored `weapons` bucket from the retired per-weapon comparison is
	// dropped here on read and gone on the next persisted write. A stored
	// `tags` bucket is the designated axis under its former name, so its
	// entries carry forward: a legacy tag IS the session name migration
	// 0018 lifted onto the session row.
	const legacyTags = arr((v as { tags?: unknown }).tags);
	return {
		mobs: arr(v.mobs),
		names: Array.from(new Set([...arr(v.names), ...legacyTags])),
	};
}

export async function initActivityArchive(): Promise<void> {
	const raw = await getPreference<unknown>(KEY, EMPTY);
	archiveState = sanitise(raw);
}

function bucketKey(kind: ArchiveKind): keyof ActivityArchiveState {
	return kind === 'mob' ? 'mobs' : 'names';
}

export async function archive(kind: ArchiveKind, name: string): Promise<void> {
	const next = { ...archiveState };
	const bucket = bucketKey(kind);
	if (next[bucket].includes(name)) return;
	next[bucket] = [...next[bucket], name];
	archiveState = next;
	await setPreference(KEY, next);
}

export async function unarchive(kind: ArchiveKind, name: string): Promise<void> {
	const next = { ...archiveState };
	const bucket = bucketKey(kind);
	if (!next[bucket].includes(name)) return;
	next[bucket] = next[bucket].filter((n) => n !== name);
	archiveState = next;
	await setPreference(KEY, next);
}

export function isArchived(state: ActivityArchiveState, kind: ArchiveKind, name: string): boolean {
	return state[bucketKey(kind)].includes(name);
}
