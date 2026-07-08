import { getPreference, setPreference } from './preferences';

export type ArchiveKind = 'mob' | 'tag' | 'weapon';

export type ActivityArchiveState = {
	mobs: string[];
	tags: string[];
	weapons: string[];
};

const KEY = 'activityArchive';

const EMPTY: ActivityArchiveState = { mobs: [], tags: [], weapons: [] };

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
	return {
		mobs: arr(v.mobs),
		tags: arr(v.tags),
		weapons: arr(v.weapons),
	};
}

export async function initActivityArchive(): Promise<void> {
	const raw = await getPreference<unknown>(KEY, EMPTY);
	archiveState = sanitise(raw);
}

function bucketKey(kind: ArchiveKind): keyof ActivityArchiveState {
	return kind === 'mob' ? 'mobs' : kind === 'tag' ? 'tags' : 'weapons';
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
