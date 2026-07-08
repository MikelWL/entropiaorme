import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// Mock the preferences seam so getPreference/setPreference are observable and
// do not touch Tauri/localStorage. getPreference is keyed by its first arg so
// initNews's parallel Promise.all reads resolve per-key.
vi.mock('./preferences', () => ({
	getPreference: vi.fn(),
	setPreference: vi.fn(),
}));

import {
	initNews,
	markNewsAsRead,
	markNewsOptInSeen,
	NEWS_PREFERENCE_KEYS,
	type NewsCache,
	type NewsEntry,
	newsCache,
	newsHasUnread,
	newsLastViewedAt,
	newsOptIn,
	newsOptInSeen,
	persistNewsCache,
	purgeNewsCache,
	setNewsOptIn,
} from './news.svelte';
import { getPreference, setPreference } from './preferences';

const getPreferenceMock = vi.mocked(getPreference);
const setPreferenceMock = vi.mocked(setPreference);

function makeEntry(overrides: Partial<NewsEntry> = {}): NewsEntry {
	return {
		slug: 'sample-slug',
		title: 'Sample title',
		date: '2026-01-01',
		category: 'article',
		body: 'Sample body.',
		...overrides,
	};
}

function makeCache(items: NewsEntry[], fetchedAt = '2026-05-01T00:00:00Z'): NewsCache {
	return { items, fetchedAt };
}

// Configure getPreference to answer per key. Any key not present resolves to
// the defaultValue passed by the caller, mirroring the real signature.
function stubPreferences(values: Record<string, unknown>): void {
	getPreferenceMock.mockImplementation(async (key: string, defaultValue: unknown) => {
		return key in values ? values[key] : defaultValue;
	});
}

beforeEach(() => {
	// Reset module-level state so tests are order-independent.
	newsOptIn.current = false;
	newsOptInSeen.current = false;
	newsCache.current = null;
	newsLastViewedAt.current = null;
	getPreferenceMock.mockReset();
	setPreferenceMock.mockReset();
	setPreferenceMock.mockResolvedValue(undefined);
});

afterEach(() => {
	vi.clearAllMocks();
});

describe('initNews', () => {
	it('discards a malformed cache value (empty object) and purges the stale shape', async () => {
		stubPreferences({
			[NEWS_PREFERENCE_KEYS.optIn]: true,
			[NEWS_PREFERENCE_KEYS.optInSeen]: true,
			[NEWS_PREFERENCE_KEYS.cache]: {},
			[NEWS_PREFERENCE_KEYS.lastViewedAt]: '2026-02-02',
		});

		await initNews();

		expect(newsCache.current).toBeNull();
		// Stale shape is purged with an explicit null write to the cache key.
		expect(setPreferenceMock).toHaveBeenCalledWith(NEWS_PREFERENCE_KEYS.cache, null);
		// opt-in / seen / lastViewed are loaded into their state.
		expect(newsOptIn.current).toBe(true);
		expect(newsOptInSeen.current).toBe(true);
		expect(newsLastViewedAt.current).toBe('2026-02-02');
	});

	it('discards a non-cache object (missing fetchedAt) and purges it', async () => {
		stubPreferences({
			[NEWS_PREFERENCE_KEYS.cache]: { items: [makeEntry()] },
		});

		await initNews();

		expect(newsCache.current).toBeNull();
		expect(setPreferenceMock).toHaveBeenCalledWith(NEWS_PREFERENCE_KEYS.cache, null);
	});

	it('loads a valid NewsCache and does NOT write a purge for the cache key', async () => {
		const cache = makeCache([makeEntry()]);
		stubPreferences({
			[NEWS_PREFERENCE_KEYS.cache]: cache,
		});

		await initNews();

		expect(newsCache.current).toEqual(cache);
		// No purge write to the cache key when the stored shape is valid.
		expect(setPreferenceMock).not.toHaveBeenCalledWith(NEWS_PREFERENCE_KEYS.cache, null);
	});

	it('leaves the cache null with no purge write when the stored cache is null', async () => {
		stubPreferences({
			[NEWS_PREFERENCE_KEYS.cache]: null,
		});

		await initNews();

		expect(newsCache.current).toBeNull();
		// null is the absence of a cache, not a stale shape: no purge write.
		expect(setPreferenceMock).not.toHaveBeenCalled();
	});

	it('loads opt-in OFF at runtime until chosen (the opt-out posture is panel-driven) and the other defaults when unset', async () => {
		stubPreferences({});

		await initNews();

		// The runtime state stays OFF until the user has made the choice, so a
		// not-yet-onboarded profile makes no news request before consent. The
		// opt-out "on by default" lives in the onboarding panel and the saved
		// preference, not in this runtime default.
		expect(getPreferenceMock).toHaveBeenCalledWith(NEWS_PREFERENCE_KEYS.optIn, false);
		expect(newsOptIn.current).toBe(false);
		expect(newsOptInSeen.current).toBe(false);
		expect(newsLastViewedAt.current).toBeNull();
		expect(newsCache.current).toBeNull();
	});
});

describe('setNewsOptIn', () => {
	it('opting out sets the flag, persists it, and purges the cache', async () => {
		newsCache.current = makeCache([makeEntry()]);

		await setNewsOptIn(false);

		expect(newsOptIn.current).toBe(false);
		expect(setPreferenceMock).toHaveBeenCalledWith(NEWS_PREFERENCE_KEYS.optIn, false);
		// Opt-out purges the cache (state cleared + null persisted).
		expect(newsCache.current).toBeNull();
		expect(setPreferenceMock).toHaveBeenCalledWith(NEWS_PREFERENCE_KEYS.cache, null);
	});

	it('opting in sets the flag, persists it, and does NOT purge the cache', async () => {
		const cache = makeCache([makeEntry()]);
		newsCache.current = cache;

		await setNewsOptIn(true);

		expect(newsOptIn.current).toBe(true);
		expect(setPreferenceMock).toHaveBeenCalledWith(NEWS_PREFERENCE_KEYS.optIn, true);
		// No purge on opt-in: cache untouched and no null write to the cache key.
		expect(newsCache.current).toEqual(cache);
		expect(setPreferenceMock).not.toHaveBeenCalledWith(NEWS_PREFERENCE_KEYS.cache, null);
	});
});

describe('markNewsOptInSeen', () => {
	it('sets the seen flag and persists it', async () => {
		await markNewsOptInSeen();

		expect(newsOptInSeen.current).toBe(true);
		expect(setPreferenceMock).toHaveBeenCalledWith(NEWS_PREFERENCE_KEYS.optInSeen, true);
	});
});

describe('purgeNewsCache', () => {
	it('clears the state and persists a null cache', async () => {
		newsCache.current = makeCache([makeEntry()]);

		await purgeNewsCache();

		expect(newsCache.current).toBeNull();
		expect(setPreferenceMock).toHaveBeenCalledWith(NEWS_PREFERENCE_KEYS.cache, null);
	});
});

describe('persistNewsCache', () => {
	it('sets the state and persists the given cache', async () => {
		const cache = makeCache([makeEntry()]);

		await persistNewsCache(cache);

		expect(newsCache.current).toEqual(cache);
		expect(setPreferenceMock).toHaveBeenCalledWith(NEWS_PREFERENCE_KEYS.cache, cache);
	});
});

describe('markNewsAsRead', () => {
	it('advances lastViewed to the newest item date and persists it', async () => {
		newsCache.current = makeCache([
			makeEntry({ slug: 'a', date: '2026-01-01' }),
			makeEntry({ slug: 'c', date: '2026-03-15' }),
			makeEntry({ slug: 'b', date: '2026-02-10' }),
		]);

		await markNewsAsRead();

		expect(newsLastViewedAt.current).toBe('2026-03-15');
		expect(setPreferenceMock).toHaveBeenCalledWith(NEWS_PREFERENCE_KEYS.lastViewedAt, '2026-03-15');
	});

	it('is a no-op when current cursor already equals the newest date', async () => {
		newsLastViewedAt.current = '2026-03-15';
		newsCache.current = makeCache([
			makeEntry({ slug: 'a', date: '2026-01-01' }),
			makeEntry({ slug: 'c', date: '2026-03-15' }),
		]);

		await markNewsAsRead();

		// current >= newest: never moves backward, no state change, no write.
		expect(newsLastViewedAt.current).toBe('2026-03-15');
		expect(setPreferenceMock).not.toHaveBeenCalled();
	});

	it('is a no-op when current cursor is already past the newest date', async () => {
		newsLastViewedAt.current = '2026-12-31';
		newsCache.current = makeCache([makeEntry({ date: '2026-03-15' })]);

		await markNewsAsRead();

		expect(newsLastViewedAt.current).toBe('2026-12-31');
		expect(setPreferenceMock).not.toHaveBeenCalled();
	});

	it('is a no-op when the cache is empty', async () => {
		newsCache.current = makeCache([]);

		await markNewsAsRead();

		expect(newsLastViewedAt.current).toBeNull();
		expect(setPreferenceMock).not.toHaveBeenCalled();
	});

	it('is a no-op when the cache is null', async () => {
		newsCache.current = null;

		await markNewsAsRead();

		expect(newsLastViewedAt.current).toBeNull();
		expect(setPreferenceMock).not.toHaveBeenCalled();
	});

	it('advances from a null cursor to the newest date', async () => {
		newsLastViewedAt.current = null;
		newsCache.current = makeCache([makeEntry({ date: '2026-04-01' })]);

		await markNewsAsRead();

		expect(newsLastViewedAt.current).toBe('2026-04-01');
		expect(setPreferenceMock).toHaveBeenCalledWith(NEWS_PREFERENCE_KEYS.lastViewedAt, '2026-04-01');
	});

	it('advances to the true max from an intermediate cursor regardless of item order', async () => {
		// Cursor sits strictly between two unordered item dates: the reduce must
		// pick the global max, not the first item greater than the cursor.
		newsLastViewedAt.current = '2026-02-10';
		newsCache.current = makeCache([
			makeEntry({ slug: 'c', date: '2026-03-15' }),
			makeEntry({ slug: 'a', date: '2026-01-01' }),
		]);

		await markNewsAsRead();

		expect(newsLastViewedAt.current).toBe('2026-03-15');
		expect(setPreferenceMock).toHaveBeenCalledWith(NEWS_PREFERENCE_KEYS.lastViewedAt, '2026-03-15');
	});

	it('is a no-op when a non-empty cache yields a falsy newest (empty dates)', async () => {
		// Degenerate guard: reduce seeds '' and uses a strict '>' predicate, so
		// all-empty dates leave newest '' and the `if (!newest) return` fires
		// even though items.length > 0.
		newsLastViewedAt.current = null;
		newsCache.current = makeCache([makeEntry({ date: '' }), makeEntry({ date: '' })]);

		await markNewsAsRead();

		expect(newsLastViewedAt.current).toBeNull();
		expect(setPreferenceMock).not.toHaveBeenCalled();
	});
});

describe('newsHasUnread (derived)', () => {
	it('is false when the cache is null', () => {
		newsCache.current = null;
		newsLastViewedAt.current = null;
		expect(newsHasUnread.current).toBe(false);
	});

	it('is false when the cache has no items', () => {
		newsCache.current = makeCache([]);
		newsLastViewedAt.current = null;
		expect(newsHasUnread.current).toBe(false);
	});

	it('is true when there are items and lastViewed is null', () => {
		newsCache.current = makeCache([makeEntry({ date: '2026-01-01' })]);
		newsLastViewedAt.current = null;
		expect(newsHasUnread.current).toBe(true);
	});

	it('is false when lastViewed equals the newest item date', () => {
		newsCache.current = makeCache([
			makeEntry({ slug: 'a', date: '2026-01-01' }),
			makeEntry({ slug: 'b', date: '2026-03-15' }),
		]);
		newsLastViewedAt.current = '2026-03-15';
		// Strict '>' compare: equality is NOT unread.
		expect(newsHasUnread.current).toBe(false);
	});

	it('is true when an item date is lexicographically greater than lastViewed', () => {
		newsCache.current = makeCache([
			makeEntry({ slug: 'a', date: '2026-01-01' }),
			makeEntry({ slug: 'b', date: '2026-03-16' }),
		]);
		newsLastViewedAt.current = '2026-03-15';
		expect(newsHasUnread.current).toBe(true);
	});

	it('is false when every item date is older than lastViewed', () => {
		newsCache.current = makeCache([makeEntry({ date: '2026-02-01' })]);
		newsLastViewedAt.current = '2026-03-15';
		expect(newsHasUnread.current).toBe(false);
	});

	it('is false when a non-empty cache yields a falsy newest (empty dates)', () => {
		// Mirrors markNewsAsRead's `!newest` guard: all-empty dates reduce to ''
		// so the derivation returns false even with items present and a null cursor.
		newsCache.current = makeCache([makeEntry({ date: '' }), makeEntry({ date: '' })]);
		newsLastViewedAt.current = null;
		expect(newsHasUnread.current).toBe(false);
	});

	it('tracks cache and cursor changes across reads', () => {
		// The getter derives from the live state on every read, so a cache or
		// cursor write is visible immediately (reactive contexts track the same
		// reads through the compiler).
		const seen: boolean[] = [];
		seen.push(newsHasUnread.current);

		newsCache.current = makeCache([makeEntry({ date: '2026-05-01' })]);
		seen.push(newsHasUnread.current);
		newsLastViewedAt.current = '2026-05-01';
		seen.push(newsHasUnread.current);

		// Initial false -> true (items, no cursor) -> false (cursor caught up).
		expect(seen).toEqual([false, true, false]);
	});
});
