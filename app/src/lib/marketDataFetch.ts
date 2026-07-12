// The market-data service client: snapshot fetch (download-only,
// ETag-revalidated) and the strictly opt-in contribution POST. Both
// speak to the one pinned origin; the CSP `connect-src` entry gates it
// at the webview boundary, `httpsFetch` enforces the rest.

import type { MarketContributionBatch } from './api/commands.gen';
import {
	marketContributionOptIn,
	marketContributorToken,
	marketDataOptIn,
	marketSnapshotCache,
	persistMarketSnapshotCache,
	type SnapshotItem,
	type SnapshotReading,
	type SnapshotReadings,
} from './marketData.svelte';
import { httpsFetch } from './outboundHttp';

const MARKET_DATA_BASE = 'https://market-data.entropiaorme.com';
const SNAPSHOT_URL = `${MARKET_DATA_BASE}/v1/latest.json`;
const SUBMISSIONS_URL = `${MARKET_DATA_BASE}/v1/submissions`;

const HORIZONS = ['day', 'week', 'month', 'year', 'decade'] as const;

function isReading(value: unknown): value is SnapshotReading {
	if (!value || typeof value !== 'object') return false;
	const r = value as Partial<SnapshotReading>;
	if (r.markupPct !== null && typeof r.markupPct !== 'number') return false;
	return typeof r.salesPed === 'number';
}

function isReadings(value: unknown): value is SnapshotReadings {
	if (!value || typeof value !== 'object') return false;
	const r = value as Record<string, unknown>;
	return HORIZONS.every((horizon) => isReading(r[horizon]));
}

function isItem(value: unknown): value is SnapshotItem {
	if (!value || typeof value !== 'object') return false;
	const i = value as Partial<SnapshotItem>;
	if (typeof i.itemName !== 'string' || !i.itemName) return false;
	if (typeof i.tier !== 'number') return false;
	if (typeof i.observedAt !== 'string') return false;
	return isReadings(i.readings);
}

type WireSnapshot = {
	schemaVersion: 1;
	generatedAt: string;
	contributorCount?: number;
	items: SnapshotItem[];
};

function isSnapshot(value: unknown): value is WireSnapshot {
	if (!value || typeof value !== 'object') return false;
	const s = value as Partial<WireSnapshot>;
	if (s.schemaVersion !== 1) return false;
	if (typeof s.generatedAt !== 'string') return false;
	if (s.contributorCount !== undefined && typeof s.contributorCount !== 'number') return false;
	return Array.isArray(s.items) && s.items.every(isItem);
}

export type RefreshResult = { ok: true; changed: boolean } | { ok: false; reason: string };

/// Fetch the published snapshot, replaying the cached ETag; a 304 keeps
/// the cache and only refreshes its fetch timestamp.
export async function fetchMarketSnapshot(): Promise<RefreshResult> {
	const cached = marketSnapshotCache.current;
	const headers: Record<string, string> = {};
	if (cached?.etag) {
		headers['If-None-Match'] = cached.etag;
	}
	try {
		const res = await httpsFetch(SNAPSHOT_URL, { headers, acceptStatus: [304] });
		if (res.status === 304 && cached) {
			await persistMarketSnapshotCache({ ...cached, fetchedAt: new Date().toISOString() });
			return { ok: true, changed: false };
		}
		const raw: unknown = await res.json();
		if (!isSnapshot(raw)) {
			return { ok: false, reason: 'snapshot schema rejected' };
		}
		await persistMarketSnapshotCache({
			generatedAt: raw.generatedAt,
			contributorCount: raw.contributorCount ?? 0,
			items: raw.items,
			fetchedAt: new Date().toISOString(),
			etag: res.headers.get('etag'),
		});
		return { ok: true, changed: true };
	} catch (err) {
		return { ok: false, reason: err instanceof Error ? err.message : String(err) };
	}
}

export async function maybeRefreshMarketSnapshotOnMount(): Promise<void> {
	if (!marketDataOptIn.current) return;
	await fetchMarketSnapshot();
}

export type ContributionResult = { ok: true; stored: number } | { ok: false; reason: string };

/// The wire submission the service's v1 schema expects, built from the
/// typed contribution batch (the user's latest accepted paste).
export function contributionWirePayload(batch: MarketContributionBatch): unknown {
	return {
		schemaVersion: 1,
		observedAt: new Date(batch.observedAt * 1000).toISOString(),
		items: batch.items.map((item) => ({
			itemName: item.itemName,
			tier: item.tier,
			readings: {
				day: item.day,
				week: item.week,
				month: item.month,
				year: item.year,
				decade: item.decade,
			},
		})),
	};
}

/// POST the latest accepted paste. Inert unless the contribution opt-in
/// is on AND a contributor token is configured; both gates re-checked
/// here so no caller can bypass them.
export async function contributeMarketBatch(
	batch: MarketContributionBatch,
): Promise<ContributionResult> {
	if (!marketContributionOptIn.current) {
		return { ok: false, reason: 'contribution is not enabled' };
	}
	const token = marketContributorToken.current.trim();
	if (!token) {
		return { ok: false, reason: 'no contributor token configured' };
	}
	try {
		const res = await httpsFetch(SUBMISSIONS_URL, {
			method: 'POST',
			headers: {
				Authorization: `Bearer ${token}`,
				'Content-Type': 'application/json',
			},
			body: JSON.stringify(contributionWirePayload(batch)),
		});
		const raw: unknown = await res.json();
		const stored =
			raw && typeof raw === 'object' && typeof (raw as { stored?: unknown }).stored === 'number'
				? (raw as { stored: number }).stored
				: batch.items.length;
		return { ok: true, stored };
	} catch (err) {
		return { ok: false, reason: err instanceof Error ? err.message : String(err) };
	}
}

export const MARKET_DATA_URLS = {
	base: MARKET_DATA_BASE,
	snapshot: SNAPSHOT_URL,
	submissions: SUBMISSIONS_URL,
} as const;
