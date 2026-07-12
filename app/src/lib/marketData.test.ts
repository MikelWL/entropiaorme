import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('./preferences', () => ({
	getPreference: vi.fn(),
	setPreference: vi.fn(),
}));

import {
	initMarketData,
	MARKET_DATA_PREFERENCE_KEYS,
	type MarketSnapshotCache,
	marketContributionOptIn,
	marketContributorToken,
	marketDataOptIn,
	marketDataOptInSeen,
	marketSnapshotCache,
	persistMarketSnapshotCache,
	purgeMarketSnapshotCache,
	setMarketContributionOptIn,
	setMarketContributorToken,
	setMarketDataOptIn,
} from './marketData.svelte';
import { getPreference, setPreference } from './preferences';

const getPreferenceMock = vi.mocked(getPreference);
const setPreferenceMock = vi.mocked(setPreference);

function makeCache(): MarketSnapshotCache {
	return {
		generatedAt: '2026-07-12T11:49:43Z',
		contributorCount: 1,
		items: [],
		fetchedAt: '2026-07-12T12:00:00Z',
		etag: '"abc"',
	};
}

function primePreferences(values: Record<string, unknown>) {
	getPreferenceMock.mockImplementation(async (key: string, fallback: unknown) =>
		key in values ? values[key] : fallback,
	);
}

beforeEach(() => {
	vi.clearAllMocks();
	marketDataOptIn.current = false;
	marketDataOptInSeen.current = false;
	marketContributionOptIn.current = false;
	marketContributorToken.current = '';
	marketSnapshotCache.current = null;
});

describe('initMarketData', () => {
	it('defaults everything off before any choice is saved', async () => {
		primePreferences({});
		await initMarketData();
		expect(marketDataOptIn.current).toBe(false);
		expect(marketDataOptInSeen.current).toBe(false);
		expect(marketContributionOptIn.current).toBe(false);
		expect(marketContributorToken.current).toBe('');
		expect(marketSnapshotCache.current).toBeNull();
	});

	it('hydrates saved choices and a well-formed cache', async () => {
		primePreferences({
			[MARKET_DATA_PREFERENCE_KEYS.fetchOptIn]: true,
			[MARKET_DATA_PREFERENCE_KEYS.optInSeen]: true,
			[MARKET_DATA_PREFERENCE_KEYS.contributeOptIn]: true,
			[MARKET_DATA_PREFERENCE_KEYS.contributorToken]: 'id.secret',
			[MARKET_DATA_PREFERENCE_KEYS.snapshotCache]: makeCache(),
		});
		await initMarketData();
		expect(marketDataOptIn.current).toBe(true);
		expect(marketContributionOptIn.current).toBe(true);
		expect(marketContributorToken.current).toBe('id.secret');
		expect(marketSnapshotCache.current?.etag).toBe('"abc"');
	});

	it('discards a cache whose items carry an earlier nested shape', async () => {
		primePreferences({
			[MARKET_DATA_PREFERENCE_KEYS.snapshotCache]: {
				...makeCache(),
				items: [{ itemName: 'x', tier: 0, observedAt: 't', readings: { day: {} } }],
			},
		});
		await initMarketData();
		expect(marketSnapshotCache.current).toBeNull();
	});

	it('discards a cache written under an earlier shape', async () => {
		primePreferences({
			[MARKET_DATA_PREFERENCE_KEYS.snapshotCache]: { some: 'older-shape' },
		});
		await initMarketData();
		expect(marketSnapshotCache.current).toBeNull();
		expect(setPreferenceMock).toHaveBeenCalledWith(MARKET_DATA_PREFERENCE_KEYS.snapshotCache, null);
	});
});

describe('opt-in setters', () => {
	it('persists the fetch opt-in and purges the cache on opt-out', async () => {
		marketSnapshotCache.current = makeCache();
		await setMarketDataOptIn(false);
		expect(marketSnapshotCache.current).toBeNull();
		expect(setPreferenceMock).toHaveBeenCalledWith(MARKET_DATA_PREFERENCE_KEYS.fetchOptIn, false);
		expect(setPreferenceMock).toHaveBeenCalledWith(MARKET_DATA_PREFERENCE_KEYS.snapshotCache, null);
	});

	it('keeps the cache on opt-in', async () => {
		marketSnapshotCache.current = makeCache();
		await setMarketDataOptIn(true);
		expect(marketSnapshotCache.current).not.toBeNull();
	});

	it('persists the contribution opt-in and token independently', async () => {
		await setMarketContributionOptIn(true);
		await setMarketContributorToken('id.secret');
		expect(marketContributionOptIn.current).toBe(true);
		expect(marketContributorToken.current).toBe('id.secret');
		expect(setPreferenceMock).toHaveBeenCalledWith(
			MARKET_DATA_PREFERENCE_KEYS.contributeOptIn,
			true,
		);
		expect(setPreferenceMock).toHaveBeenCalledWith(
			MARKET_DATA_PREFERENCE_KEYS.contributorToken,
			'id.secret',
		);
	});
});

describe('cache persistence', () => {
	it('round-trips persist and purge', async () => {
		const cache = makeCache();
		await persistMarketSnapshotCache(cache);
		expect(marketSnapshotCache.current).toEqual(cache);
		await purgeMarketSnapshotCache();
		expect(marketSnapshotCache.current).toBeNull();
	});
});
