import {
	type NewsCache,
	type NewsEntry,
	type NewsFeed,
	newsOptIn,
	persistNewsCache,
} from './news.svelte';

import { httpsFetch } from './outboundHttp';

// The news feed client. All outbound HTTP routes through the hardened
// gateway in outboundHttp.ts; the CSP `connect-src` entry in
// app/src-tauri/entropia-orme/tauri.conf.json gates the host allowlist
// at the webview boundary, and this constant declares this feature's
// pinned origin.
const NEWS_SOURCE_BASE = 'https://entropiaorme.com';
const FEED_URL = `${NEWS_SOURCE_BASE}/news.json`;

function isEntry(value: unknown): value is NewsEntry {
	if (!value || typeof value !== 'object') return false;
	const e = value as Partial<NewsEntry>;
	if (typeof e.slug !== 'string' || !e.slug) return false;
	if (typeof e.title !== 'string') return false;
	if (typeof e.date !== 'string' || !e.date) return false;
	if (e.category !== 'article' && e.category !== 'changelog') return false;
	if (typeof e.body !== 'string') return false;
	if (e.dek !== undefined && typeof e.dek !== 'string') return false;
	if (e.eyebrow !== undefined && typeof e.eyebrow !== 'string') return false;
	if (e.hero !== undefined && typeof e.hero !== 'string') return false;
	if (e.link !== undefined && typeof e.link !== 'string') return false;
	if (
		e.pin_slot !== undefined &&
		e.pin_slot !== 'community' &&
		e.pin_slot !== 'release' &&
		e.pin_slot !== 'foundations'
	)
		return false;
	if (e.pin_blurb !== undefined && typeof e.pin_blurb !== 'string') return false;
	if (e.pin_icon !== undefined && typeof e.pin_icon !== 'string') return false;
	if (e.pin_cta !== undefined && typeof e.pin_cta !== 'string') return false;
	return true;
}

function isFeed(value: unknown): value is NewsFeed {
	if (!value || typeof value !== 'object') return false;
	const v = value as { items?: unknown };
	return Array.isArray(v.items) && v.items.every(isEntry);
}

export async function fetchNews(): Promise<NewsCache> {
	const res = await httpsFetch(FEED_URL);
	const raw: unknown = await res.json();
	if (!isFeed(raw)) {
		throw new Error('feed schema rejected');
	}
	return {
		items: raw.items,
		fetchedAt: new Date().toISOString(),
	};
}

export type RefreshResult = { ok: true } | { ok: false; reason: string };

export async function refreshNews(): Promise<RefreshResult> {
	if (!newsOptIn.current) {
		return { ok: false, reason: 'opt-in is off' };
	}
	try {
		const cache = await fetchNews();
		await persistNewsCache(cache);
		return { ok: true };
	} catch (err) {
		return {
			ok: false,
			reason: err instanceof Error ? err.message : String(err),
		};
	}
}

export async function maybeRefreshOnMount(): Promise<void> {
	if (!newsOptIn.current) return;
	await refreshNews();
}
