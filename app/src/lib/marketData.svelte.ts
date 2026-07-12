// Shared market-data state: the two consent gates and the cached
// snapshot. Two independent choices, deliberately separate:
//
//   - Fetching the shared snapshot (download-only, like the news feed):
//     offered in onboarding, opt-out posture once chosen.
//   - Contributing pasted observations (the app's only user-initiated
//     upload): strictly opt-in, DEFAULT OFF, configured in Settings
//     only, and inert without a contributor token.
//
// Snapshot markup stays on the informational market layer: estimates
// never join the ledger or any realised figure.

import { getPreference, setPreference } from './preferences';

export type SnapshotReading = {
	markupPct: number | null;
	salesPed: number;
};

export type SnapshotReadings = {
	day: SnapshotReading;
	week: SnapshotReading;
	month: SnapshotReading;
	year: SnapshotReading;
	decade: SnapshotReading;
};

export type SnapshotItem = {
	itemName: string;
	tier: number;
	/** RFC 3339; when the newest underlying observation was taken. */
	observedAt: string;
	readings: SnapshotReadings;
};

export type MarketSnapshotCache = {
	/** RFC 3339; when the service generated the snapshot. */
	generatedAt: string;
	contributorCount: number;
	items: SnapshotItem[];
	/** When this client fetched it (ISO). */
	fetchedAt: string;
	/** The snapshot's HTTP ETag, replayed as If-None-Match. */
	etag: string | null;
};

const KEY_FETCH_OPT_IN = 'market_data_opt_in';
const KEY_OPT_IN_SEEN = 'market_data_opt_in_seen';
const KEY_CONTRIBUTE_OPT_IN = 'market_contribution_opt_in';
const KEY_CONTRIBUTOR_TOKEN = 'market_contributor_token';
const KEY_SNAPSHOT_CACHE = 'market_snapshot_cache';

// Runtime default OFF until the user has made the choice, mirroring the
// news feed: no request fires before consent has been seen and saved.
let fetchOptIn = $state(false);
let optInSeen = $state(false);
// Contribution is default OFF at every layer; only an explicit Settings
// action turns it on.
let contributeOptIn = $state(false);
let contributorToken = $state('');
let cache = $state<MarketSnapshotCache | null>(null);

export const marketDataOptIn = {
	get current(): boolean {
		return fetchOptIn;
	},
	set current(value: boolean) {
		fetchOptIn = value;
	},
};

export const marketDataOptInSeen = {
	get current(): boolean {
		return optInSeen;
	},
	set current(value: boolean) {
		optInSeen = value;
	},
};

export const marketContributionOptIn = {
	get current(): boolean {
		return contributeOptIn;
	},
	set current(value: boolean) {
		contributeOptIn = value;
	},
};

export const marketContributorToken = {
	get current(): string {
		return contributorToken;
	},
	set current(value: string) {
		contributorToken = value;
	},
};

export const marketSnapshotCache = {
	get current(): MarketSnapshotCache | null {
		return cache;
	},
	set current(value: MarketSnapshotCache | null) {
		cache = value;
	},
};

function isCache(value: unknown): value is MarketSnapshotCache {
	if (!value || typeof value !== 'object') return false;
	const c = value as Partial<MarketSnapshotCache>;
	if (typeof c.generatedAt !== 'string') return false;
	if (typeof c.contributorCount !== 'number') return false;
	if (typeof c.fetchedAt !== 'string') return false;
	if (c.etag !== null && typeof c.etag !== 'string') return false;
	return Array.isArray(c.items);
}

export async function initMarketData(): Promise<void> {
	const [storedOptIn, seen, storedContribute, token, rawCache] = await Promise.all([
		getPreference<boolean>(KEY_FETCH_OPT_IN, false),
		getPreference<boolean>(KEY_OPT_IN_SEEN, false),
		getPreference<boolean>(KEY_CONTRIBUTE_OPT_IN, false),
		getPreference<string>(KEY_CONTRIBUTOR_TOKEN, ''),
		getPreference<unknown>(KEY_SNAPSHOT_CACHE, null),
	]);
	fetchOptIn = storedOptIn;
	optInSeen = seen;
	contributeOptIn = storedContribute;
	contributorToken = token;
	// Discard caches written under any earlier shape; opted-in users
	// repopulate on the next refresh.
	cache = isCache(rawCache) ? rawCache : null;
	if (!isCache(rawCache) && rawCache !== null) {
		await setPreference<MarketSnapshotCache | null>(KEY_SNAPSHOT_CACHE, null);
	}
}

export async function setMarketDataOptIn(value: boolean): Promise<void> {
	fetchOptIn = value;
	await setPreference(KEY_FETCH_OPT_IN, value);
	if (!value) {
		await purgeMarketSnapshotCache();
	}
}

export async function markMarketDataOptInSeen(): Promise<void> {
	optInSeen = true;
	await setPreference(KEY_OPT_IN_SEEN, true);
}

export async function setMarketContributionOptIn(value: boolean): Promise<void> {
	contributeOptIn = value;
	await setPreference(KEY_CONTRIBUTE_OPT_IN, value);
}

export async function setMarketContributorToken(value: string): Promise<void> {
	contributorToken = value;
	await setPreference(KEY_CONTRIBUTOR_TOKEN, value);
}

export async function purgeMarketSnapshotCache(): Promise<void> {
	cache = null;
	await setPreference<MarketSnapshotCache | null>(KEY_SNAPSHOT_CACHE, null);
}

export async function persistMarketSnapshotCache(value: MarketSnapshotCache): Promise<void> {
	cache = value;
	await setPreference(KEY_SNAPSHOT_CACHE, value);
}

export const MARKET_DATA_PREFERENCE_KEYS = {
	fetchOptIn: KEY_FETCH_OPT_IN,
	optInSeen: KEY_OPT_IN_SEEN,
	contributeOptIn: KEY_CONTRIBUTE_OPT_IN,
	contributorToken: KEY_CONTRIBUTOR_TOKEN,
	snapshotCache: KEY_SNAPSHOT_CACHE,
} as const;
