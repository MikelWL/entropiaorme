import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// The `./preferences` seam is mocked so the suite never touches Tauri or
// localStorage: getPreference feeds the persisted-shape fixtures into
// initActivityArchive, and setPreference is a spy asserting persistence
// behaviour (call count / payload). Both are reset per test.
const getPreference = vi.fn();
const setPreference = vi.fn();

vi.mock('./preferences', () => ({
	getPreference: (...args: unknown[]) => getPreference(...args),
	setPreference: (...args: unknown[]) => setPreference(...args),
}));

// The state lives at module scope ($state seeded with EMPTY), so each test
// re-imports a fresh module graph via resetModules + dynamic import to stay
// order-independent.
type ArchiveModule = typeof import('./activityArchive.svelte');

async function freshModule(): Promise<ArchiveModule> {
	vi.resetModules();
	return import('./activityArchive.svelte');
}

beforeEach(() => {
	getPreference.mockReset();
	setPreference.mockReset();
	setPreference.mockResolvedValue(undefined);
});

afterEach(() => {
	vi.clearAllMocks();
});

describe('sanitise (via initActivityArchive)', () => {
	it('coerces null from the store into the empty shape', async () => {
		getPreference.mockResolvedValue(null);
		const mod = await freshModule();
		await mod.initActivityArchive();
		expect(mod.activityArchive.current).toEqual({ mobs: [], names: [] });
	});

	it('coerces undefined from the store into the empty shape', async () => {
		getPreference.mockResolvedValue(undefined);
		const mod = await freshModule();
		await mod.initActivityArchive();
		expect(mod.activityArchive.current).toEqual({ mobs: [], names: [] });
	});

	it('drops a stored weapons bucket from the retired per-weapon comparison', async () => {
		getPreference.mockResolvedValue({
			mobs: ['atrox'],
			names: [],
			weapons: ['ArMatrix LR-69'],
		});
		const mod = await freshModule();
		await mod.initActivityArchive();
		expect(mod.activityArchive.current).toEqual({ mobs: ['atrox'], names: [] });
	});

	it('replaces a non-array bucket with an empty array', async () => {
		getPreference.mockResolvedValue({
			mobs: 'not-an-array',
			names: 42,
		});
		const mod = await freshModule();
		await mod.initActivityArchive();
		expect(mod.activityArchive.current).toEqual({ mobs: [], names: [] });
	});

	it('filters out non-string members from a bucket', async () => {
		getPreference.mockResolvedValue({
			mobs: ['atrox', 1, null, undefined, { x: 1 }, 'molisk', true],
			names: [],
		});
		const mod = await freshModule();
		await mod.initActivityArchive();
		expect(mod.activityArchive.current.mobs).toEqual(['atrox', 'molisk']);
	});

	it('dedupes members via Set, preserving first-occurrence order', async () => {
		getPreference.mockResolvedValue({
			mobs: ['atrox', 'molisk', 'atrox', 'daikiba', 'molisk'],
			names: [],
		});
		const mod = await freshModule();
		await mod.initActivityArchive();
		expect(mod.activityArchive.current.mobs).toEqual(['atrox', 'molisk', 'daikiba']);
	});

	it('passes the storage KEY and EMPTY default through to getPreference', async () => {
		getPreference.mockResolvedValue(null);
		const mod = await freshModule();
		await mod.initActivityArchive();
		expect(getPreference).toHaveBeenCalledTimes(1);
		expect(getPreference).toHaveBeenCalledWith('activityArchive', {
			mobs: [],
			names: [],
		});
	});
});

describe('archive', () => {
	it('maps each ArchiveKind to its bucket and persists once', async () => {
		getPreference.mockResolvedValue(null);
		const mod = await freshModule();
		await mod.initActivityArchive();

		await mod.archive('mob', 'atrox');
		await mod.archive('name', 'event:beacon');

		expect(mod.activityArchive.current).toEqual({
			mobs: ['atrox'],
			names: ['event:beacon'],
		});
		expect(setPreference).toHaveBeenCalledTimes(2);
	});

	it('persists the new state object under the storage KEY', async () => {
		getPreference.mockResolvedValue(null);
		const mod = await freshModule();
		await mod.initActivityArchive();

		await mod.archive('mob', 'atrox');

		expect(setPreference).toHaveBeenCalledTimes(1);
		expect(setPreference).toHaveBeenCalledWith('activityArchive', {
			mobs: ['atrox'],
			names: [],
		});
	});

	it('appends to the end of an existing bucket', async () => {
		getPreference.mockResolvedValue({ mobs: ['atrox'], names: [] });
		const mod = await freshModule();
		await mod.initActivityArchive();

		await mod.archive('mob', 'molisk');

		expect(mod.activityArchive.current.mobs).toEqual(['atrox', 'molisk']);
	});

	it('short-circuits on an already-present name: no state change, no persist', async () => {
		getPreference.mockResolvedValue({ mobs: ['atrox'], names: [] });
		const mod = await freshModule();
		await mod.initActivityArchive();
		const before = mod.activityArchive.current;

		await mod.archive('mob', 'atrox');

		// Reference identity is unchanged because the state was never reassigned.
		expect(mod.activityArchive.current).toBe(before);
		expect(setPreference).not.toHaveBeenCalled();
	});
});

describe('unarchive', () => {
	it('removes a present name, persists once, and keeps siblings', async () => {
		getPreference.mockResolvedValue({
			mobs: ['atrox', 'molisk', 'daikiba'],
			names: [],
		});
		const mod = await freshModule();
		await mod.initActivityArchive();

		await mod.unarchive('mob', 'molisk');

		expect(mod.activityArchive.current.mobs).toEqual(['atrox', 'daikiba']);
		expect(setPreference).toHaveBeenCalledTimes(1);
		expect(setPreference).toHaveBeenCalledWith('activityArchive', {
			mobs: ['atrox', 'daikiba'],
			names: [],
		});
	});

	it('short-circuits on an absent name: no state change, no persist', async () => {
		getPreference.mockResolvedValue({ mobs: ['atrox'], names: [] });
		const mod = await freshModule();
		await mod.initActivityArchive();
		const before = mod.activityArchive.current;

		await mod.unarchive('mob', 'nonexistent');

		expect(mod.activityArchive.current).toBe(before);
		expect(setPreference).not.toHaveBeenCalled();
	});

	it('routes removal to the kind-specific bucket', async () => {
		getPreference.mockResolvedValue({
			mobs: ['shared'],
			names: ['shared'],
		});
		const mod = await freshModule();
		await mod.initActivityArchive();

		await mod.unarchive('name', 'shared');

		expect(mod.activityArchive.current).toEqual({
			mobs: ['shared'],
			names: [],
		});
	});
});

describe('immutability', () => {
	it('archive produces a new state object reference', async () => {
		getPreference.mockResolvedValue(null);
		const mod = await freshModule();
		await mod.initActivityArchive();
		const before = mod.activityArchive.current;

		await mod.archive('mob', 'atrox');

		const after = mod.activityArchive.current;
		expect(after).not.toBe(before);
		// The original snapshot is not mutated in place.
		expect(before.mobs).toEqual([]);
	});

	it('archive produces a new bucket array reference (no in-place push)', async () => {
		getPreference.mockResolvedValue({ mobs: ['atrox'], names: [] });
		const mod = await freshModule();
		await mod.initActivityArchive();
		const before = mod.activityArchive.current;
		const beforeMobs = before.mobs;

		await mod.archive('mob', 'molisk');

		expect(mod.activityArchive.current.mobs).not.toBe(beforeMobs);
		expect(beforeMobs).toEqual(['atrox']);
	});

	it('unarchive produces a new state object reference', async () => {
		getPreference.mockResolvedValue({ mobs: ['atrox'], names: [] });
		const mod = await freshModule();
		await mod.initActivityArchive();
		const before = mod.activityArchive.current;

		await mod.unarchive('mob', 'atrox');

		const after = mod.activityArchive.current;
		expect(after).not.toBe(before);
		expect(before.mobs).toEqual(['atrox']);
	});
});

describe('isArchived', () => {
	it('reports membership for the matching bucket', async () => {
		const mod = await freshModule();
		const state = { mobs: ['atrox'], names: ['event:beacon'] };
		expect(mod.isArchived(state, 'mob', 'atrox')).toBe(true);
		expect(mod.isArchived(state, 'name', 'event:beacon')).toBe(true);
	});

	it('returns false for a name absent from the queried bucket', async () => {
		const mod = await freshModule();
		const state = { mobs: ['atrox'], names: [] };
		expect(mod.isArchived(state, 'mob', 'molisk')).toBe(false);
	});

	it('isolates buckets: a mob name is not archived under the designated axis', async () => {
		const mod = await freshModule();
		const state = { mobs: ['atrox'], names: [] };
		expect(mod.isArchived(state, 'mob', 'atrox')).toBe(true);
		expect(mod.isArchived(state, 'name', 'atrox')).toBe(false);
	});
});
