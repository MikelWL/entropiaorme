import { getPreference, setPreference } from './preferences';

export type NewsCategory = 'article' | 'changelog';

// Three-slot pinned-cards architecture in /news. Each slot is a named role
// with its own visual register; each slot holds at most one article;
// replacement within a slot is automatic by date. Slot defaults (icon,
// label, CTA copy) live in $lib/news-pins; per-article frontmatter
// overrides slot defaults where needed (pin_blurb, pin_icon, pin_cta).
// Release slot auto-derives from latest `category: changelog` entry when
// no article explicitly claims it.
export type SlotId = 'community' | 'release' | 'foundations';

export type NewsEntry = {
	slug: string;
	title: string;
	date: string;
	category: NewsCategory;
	body: string;
	dek?: string;
	eyebrow?: string;
	hero?: string;
	link?: string;
	pin_slot?: SlotId;
	pin_blurb?: string;
	pin_icon?: string;
	pin_cta?: string;
};

export type NewsFeed = {
	items: NewsEntry[];
};

export type NewsCache = {
	items: NewsEntry[];
	fetchedAt: string;
};

const KEY_OPT_IN = 'news_opt_in';
const KEY_OPT_IN_SEEN = 'news_opt_in_seen';
const KEY_CACHE = 'news_cache';
const KEY_LAST_VIEWED_AT = 'news_last_viewed_at';

// Runtime default OFF until the user has made the choice. The opt-out posture
// (news on by default, the user opts out in onboarding) is carried by the
// onboarding panel's default-on toggle and the saved preference, NOT by this
// runtime default: the state stays off until the choice is saved, so no news
// request fires before the user has seen the choice (a not-yet-onboarded
// profile must not phone home before consent).
let optIn = $state(false);
let optInSeen = $state(false);
let cache = $state<NewsCache | null>(null);
// Cursor for the unread-dot derivation: the highest article date the user
// has acknowledged by visiting /news. Compared lex-greater against article
// dates in the cache; persisted as an ISO string. Article-date rather than
// wall-clock-now, because articles can be stamped to a planned future
// release date (e.g. a launch-day announcement dated tomorrow), and a
// wall-clock cursor would trail those dates forever.
let lastViewedAt = $state<string | null>(null);

export const newsOptIn = {
	get current(): boolean {
		return optIn;
	},
	set current(value: boolean) {
		optIn = value;
	},
};

export const newsOptInSeen = {
	get current(): boolean {
		return optInSeen;
	},
	set current(value: boolean) {
		optInSeen = value;
	},
};

export const newsCache = {
	get current(): NewsCache | null {
		return cache;
	},
	set current(value: NewsCache | null) {
		cache = value;
	},
};

export const newsLastViewedAt = {
	get current(): string | null {
		return lastViewedAt;
	},
	set current(value: string | null) {
		lastViewedAt = value;
	},
};

function isCache(value: unknown): value is NewsCache {
	if (!value || typeof value !== 'object') return false;
	const c = value as Partial<NewsCache>;
	if (typeof c.fetchedAt !== 'string') return false;
	if (!Array.isArray(c.items)) return false;
	return true;
}

export async function initNews(): Promise<void> {
	const [storedOptIn, seen, rawCache, lastViewed] = await Promise.all([
		getPreference<boolean>(KEY_OPT_IN, false),
		getPreference<boolean>(KEY_OPT_IN_SEEN, false),
		getPreference<unknown>(KEY_CACHE, null),
		getPreference<string | null>(KEY_LAST_VIEWED_AT, null),
	]);
	optIn = storedOptIn;
	optInSeen = seen;
	// Discard caches written under any earlier shape; opt-in users will
	// repopulate on the next refresh.
	cache = isCache(rawCache) ? rawCache : null;
	if (!isCache(rawCache) && rawCache !== null) {
		await setPreference<NewsCache | null>(KEY_CACHE, null);
	}
	lastViewedAt = lastViewed;
}

export async function setNewsOptIn(value: boolean): Promise<void> {
	optIn = value;
	await setPreference(KEY_OPT_IN, value);
	if (!value) {
		await purgeNewsCache();
	}
}

export async function markNewsOptInSeen(): Promise<void> {
	optInSeen = true;
	await setPreference(KEY_OPT_IN_SEEN, true);
}

export async function purgeNewsCache(): Promise<void> {
	cache = null;
	await setPreference<NewsCache | null>(KEY_CACHE, null);
}

export async function persistNewsCache(value: NewsCache): Promise<void> {
	cache = value;
	await setPreference(KEY_CACHE, value);
}

export async function markNewsAsRead(): Promise<void> {
	if (!cache || cache.items.length === 0) return;
	const newest = cache.items.reduce((max, e) => (e.date > max ? e.date : max), '');
	if (!newest) return;
	if (lastViewedAt && lastViewedAt >= newest) return;
	lastViewedAt = newest;
	await setPreference(KEY_LAST_VIEWED_AT, newest);
}

// True when the cache contains an entry strictly newer than the last viewed
// timestamp. Acts as a binary unread indicator for the sidebar.
export const newsHasUnread = {
	get current(): boolean {
		if (!cache || cache.items.length === 0) return false;
		const newest = cache.items.reduce((max, e) => (e.date > max ? e.date : max), '');
		if (!newest) return false;
		if (!lastViewedAt) return true;
		return newest > lastViewedAt;
	},
};

export const NEWS_PREFERENCE_KEYS = {
	optIn: KEY_OPT_IN,
	optInSeen: KEY_OPT_IN_SEEN,
	cache: KEY_CACHE,
	lastViewedAt: KEY_LAST_VIEWED_AT,
} as const;
