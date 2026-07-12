import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('./preferences', () => ({
	getPreference: vi.fn(),
	setPreference: vi.fn(),
}));
vi.mock('./outboundHttp', () => ({
	httpsFetch: vi.fn(),
}));

import type { MarketContributionBatch } from './api/commands.gen';
import {
	type MarketSnapshotCache,
	marketContributionOptIn,
	marketContributorToken,
	marketDataOptIn,
	marketSnapshotCache,
} from './marketData.svelte';
import {
	contributeMarketBatch,
	contributionWirePayload,
	fetchMarketSnapshot,
	maybeRefreshMarketSnapshotOnMount,
} from './marketDataFetch';
import { httpsFetch } from './outboundHttp';

const httpsFetchMock = vi.mocked(httpsFetch);

const reading = { markupPct: 106.88, salesPed: 451.9 };
const readings = { day: reading, week: reading, month: reading, year: reading, decade: reading };

function wireSnapshot(overrides: Record<string, unknown> = {}): Record<string, unknown> {
	return {
		schemaVersion: 1,
		generatedAt: '2026-07-12T11:49:43Z',
		contributorCount: 1,
		items: [{ itemName: 'Carabok Hide', tier: 0, observedAt: '2026-07-12T11:00:00Z', readings }],
		...overrides,
	};
}

function response(
	status: number,
	body: unknown,
	etag: string | null = '"abc"',
	rawText?: string,
): Response {
	return {
		status,
		ok: status >= 200 && status < 300,
		headers: { get: (name: string) => (name.toLowerCase() === 'etag' ? etag : null) },
		json: async () => body,
		text: async () => rawText ?? JSON.stringify(body),
	} as unknown as Response;
}

function batch(): MarketContributionBatch {
	return {
		observedAt: 1_752_318_000,
		items: [
			{
				itemName: 'Carabok Hide',
				tier: 0,
				day: reading,
				week: reading,
				month: reading,
				year: reading,
				decade: reading,
			},
		],
	};
}

beforeEach(() => {
	vi.clearAllMocks();
	marketDataOptIn.current = false;
	marketContributionOptIn.current = false;
	marketContributorToken.current = '';
	marketSnapshotCache.current = null;
});

describe('fetchMarketSnapshot', () => {
	it('refuses a direct call while the opt-in is off', async () => {
		const result = await fetchMarketSnapshot();
		expect(result).toEqual({ ok: false, reason: 'opt-in is off' });
		expect(httpsFetchMock).not.toHaveBeenCalled();
	});

	it('stores a valid snapshot with its etag', async () => {
		marketDataOptIn.current = true;
		httpsFetchMock.mockResolvedValue(response(200, wireSnapshot()));
		const result = await fetchMarketSnapshot();
		expect(result).toEqual({ ok: true, changed: true });
		expect(marketSnapshotCache.current?.items).toHaveLength(1);
		expect(marketSnapshotCache.current?.etag).toBe('"abc"');
		expect(marketSnapshotCache.current?.contributorCount).toBe(1);
	});

	it('replays the cached etag and keeps the cache on a 304', async () => {
		marketDataOptIn.current = true;
		const cached: MarketSnapshotCache = {
			generatedAt: '2026-07-12T11:49:43Z',
			contributorCount: 1,
			items: [],
			fetchedAt: '2026-07-12T12:00:00Z',
			etag: '"abc"',
		};
		marketSnapshotCache.current = cached;
		httpsFetchMock.mockResolvedValue(response(304, null, null));
		const result = await fetchMarketSnapshot();
		expect(result).toEqual({ ok: true, changed: false });
		const [, init] = httpsFetchMock.mock.calls[0];
		expect(init?.headers?.['If-None-Match']).toBe('"abc"');
		expect(marketSnapshotCache.current?.generatedAt).toBe(cached.generatedAt);
	});

	it('rejects an oversized response body', async () => {
		marketDataOptIn.current = true;
		httpsFetchMock.mockResolvedValue(response(200, null, '"abc"', 'x'.repeat(4_000_001)));
		const result = await fetchMarketSnapshot();
		expect(result).toEqual({ ok: false, reason: 'snapshot rejected: response too large' });
		expect(marketSnapshotCache.current).toBeNull();
	});

	it('rejects a snapshot with an absurd item count', async () => {
		marketDataOptIn.current = true;
		const item = { itemName: 'x', tier: 0, observedAt: '2026-07-12T11:00:00Z', readings };
		httpsFetchMock.mockResolvedValue(
			response(200, wireSnapshot({ items: Array(10_001).fill(item) })),
		);
		const result = await fetchMarketSnapshot();
		expect(result).toEqual({ ok: false, reason: 'snapshot rejected: too many items' });
		expect(marketSnapshotCache.current).toBeNull();
	});

	it('rejects a payload that fails the snapshot guard', async () => {
		marketDataOptIn.current = true;
		httpsFetchMock.mockResolvedValue(response(200, wireSnapshot({ schemaVersion: 2 })));
		const result = await fetchMarketSnapshot();
		expect(result).toEqual({ ok: false, reason: 'snapshot schema rejected' });
		expect(marketSnapshotCache.current).toBeNull();
	});

	it('reports transport failures without touching the cache', async () => {
		marketDataOptIn.current = true;
		httpsFetchMock.mockRejectedValue(new Error('HTTP 500 for url'));
		const result = await fetchMarketSnapshot();
		expect(result).toEqual({ ok: false, reason: 'HTTP 500 for url' });
	});
});

describe('maybeRefreshMarketSnapshotOnMount', () => {
	it('makes no request while the opt-in is off', async () => {
		await maybeRefreshMarketSnapshotOnMount();
		expect(httpsFetchMock).not.toHaveBeenCalled();
	});

	it('fetches once opted in', async () => {
		marketDataOptIn.current = true;
		httpsFetchMock.mockResolvedValue(response(200, wireSnapshot()));
		await maybeRefreshMarketSnapshotOnMount();
		expect(httpsFetchMock).toHaveBeenCalledTimes(1);
	});
});

describe('contributionWirePayload', () => {
	it('builds the v1 wire shape with an ISO timestamp and nested readings', () => {
		const payload = contributionWirePayload(batch()) as {
			schemaVersion: number;
			observedAt: string;
			items: { itemName: string; readings: Record<string, unknown> }[];
		};
		expect(payload.schemaVersion).toBe(1);
		expect(payload.observedAt).toBe(new Date(1_752_318_000 * 1000).toISOString());
		expect(payload.items[0].itemName).toBe('Carabok Hide');
		expect(Object.keys(payload.items[0].readings)).toEqual([
			'day',
			'week',
			'month',
			'year',
			'decade',
		]);
	});
});

describe('contributeMarketBatch', () => {
	it('refuses while the contribution opt-in is off', async () => {
		marketContributorToken.current = 'id.secret';
		const result = await contributeMarketBatch(batch());
		expect(result).toEqual({ ok: false, reason: 'contribution is not enabled' });
		expect(httpsFetchMock).not.toHaveBeenCalled();
	});

	it('refuses without a contributor token', async () => {
		marketContributionOptIn.current = true;
		const result = await contributeMarketBatch(batch());
		expect(result).toEqual({ ok: false, reason: 'no contributor token configured' });
		expect(httpsFetchMock).not.toHaveBeenCalled();
	});

	it('POSTs the batch with the bearer token once both gates are open', async () => {
		marketContributionOptIn.current = true;
		marketContributorToken.current = 'id.secret';
		httpsFetchMock.mockResolvedValue(response(200, { stored: 1 }));
		const result = await contributeMarketBatch(batch());
		expect(result).toEqual({ ok: true, stored: 1 });
		const [url, init] = httpsFetchMock.mock.calls[0];
		expect(url).toBe('https://market-data.entropiaorme.com/v1/submissions');
		expect(init?.method).toBe('POST');
		expect(init?.headers?.Authorization).toBe('Bearer id.secret');
	});

	it('surfaces a rejected submission as a failure', async () => {
		marketContributionOptIn.current = true;
		marketContributorToken.current = 'id.secret';
		httpsFetchMock.mockRejectedValue(new Error('HTTP 401 for url'));
		const result = await contributeMarketBatch(batch());
		expect(result).toEqual({ ok: false, reason: 'HTTP 401 for url' });
	});
});
