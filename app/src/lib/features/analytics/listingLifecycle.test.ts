import { describe, expect, it } from 'vitest';
import type { AuctionListing } from '$lib/types/analytics';
import { hasRunOut, runOutCount } from './listingLifecycle';

function listing(overrides: Partial<AuctionListing> = {}): AuctionListing {
	return {
		id: 'listing-1',
		itemName: 'Long Moonleaf Board',
		quantity: 10,
		attributedQty: 10,
		unattributedQty: 0,
		ttValue: 10,
		attributedTt: 10,
		startingBid: 10.25,
		buyout: null,
		listingFee: 0.5,
		listedAt: '2026-08-01',
		status: 'pending',
		finalPrice: null,
		saleFee: null,
		resolvedAt: null,
		subjectKind: 'loot',
		inventoryItemId: null,
		costBasis: null,
		channel: 'auction',
		auctionDays: 7,
		expiresAt: '2026-08-08',
		activityNetMarkup: null,
		grossMarkup: null,
		...overrides,
	} as AuctionListing;
}

describe('listing lifecycle', () => {
	it('raises the question once the listing has passed its day', () => {
		expect(hasRunOut(listing(), '2026-08-09')).toBe(true);
	});

	it('stays quiet on the expiry day itself', () => {
		// The listing is still live for the whole of its final day.
		expect(hasRunOut(listing(), '2026-08-08')).toBe(false);
		expect(hasRunOut(listing(), '2026-08-02')).toBe(false);
	});

	it('never asks about a listing whose duration was not recorded', () => {
		expect(hasRunOut(listing({ auctionDays: null, expiresAt: null }), '2027-01-01')).toBe(false);
	});

	it('never asks about a listing already resolved either way', () => {
		expect(hasRunOut(listing({ status: 'sold' }), '2026-08-09')).toBe(false);
		expect(hasRunOut(listing({ status: 'expired' }), '2026-08-09')).toBe(false);
	});

	it('counts only the listings actually waiting on an answer', () => {
		const rows = [
			listing({ id: 'a' }),
			listing({ id: 'b', expiresAt: '2026-09-01' }),
			listing({ id: 'c', status: 'sold' }),
			listing({ id: 'd', expiresAt: null, auctionDays: null }),
		];
		expect(runOutCount(rows, '2026-08-09')).toBe(1);
	});
});
