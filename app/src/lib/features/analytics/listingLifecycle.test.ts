import { describe, expect, it } from 'vitest';
import type { AuctionListing } from '$lib/types/analytics';
import { hasRunOut, runOutCount } from './listingLifecycle';

/** The expiry as the backend renders it: a UTC instant, not a bare date.
 * Posted at 18:20 and run for seven days, it ends at 18:20. */
const EXPIRY = '2026-08-08T18:20:00+00:00';
const at = (iso: string) => Date.parse(iso);

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
		expiresAt: EXPIRY,
		activityNetMarkup: null,
		grossMarkup: null,
		...overrides,
	} as AuctionListing;
}

describe('listing lifecycle', () => {
	it('raises the question once the listing has passed its moment', () => {
		expect(hasRunOut(listing(), at('2026-08-08T18:20:01+00:00'))).toBe(true);
		expect(hasRunOut(listing(), at('2026-08-09T09:00:00+00:00'))).toBe(true);
	});

	it('stays quiet while the listing is still running', () => {
		// A minute short of the deadline is still a live auction, even though
		// the calendar date already matches.
		expect(hasRunOut(listing(), at('2026-08-08T18:19:00+00:00'))).toBe(false);
		expect(hasRunOut(listing(), at('2026-08-08T00:01:00+00:00'))).toBe(false);
		expect(hasRunOut(listing(), at('2026-08-02T12:00:00+00:00'))).toBe(false);
	});

	it('asks at the deadline itself, not a day later', () => {
		expect(hasRunOut(listing(), at('2026-08-08T18:20:00+00:00'))).toBe(true);
	});

	// The defect this guards: comparing a UTC expiry against a local calendar
	// date asks the question the moment local midnight passes, which is before
	// the auction has ended for anyone east of UTC. Both instants below fall on
	// a later local date than the expiry's UTC date, and both are still live.
	it('does not ask early for a deadline that straddles midnight UTC', () => {
		const straddling = listing({ expiresAt: '2026-08-08T23:30:00+00:00' });
		expect(hasRunOut(straddling, at('2026-08-08T23:29:00+00:00'))).toBe(false);
		expect(hasRunOut(straddling, at('2026-08-08T22:00:00+00:00'))).toBe(false);
		expect(hasRunOut(straddling, at('2026-08-08T23:31:00+00:00'))).toBe(true);
	});

	it('never asks about a listing whose duration was not recorded', () => {
		expect(
			hasRunOut(listing({ auctionDays: null, expiresAt: null }), at('2027-01-01T00:00:00+00:00')),
		).toBe(false);
	});

	it('stays quiet rather than guessing when the stamp cannot be read', () => {
		expect(
			hasRunOut(listing({ expiresAt: 'not a timestamp' }), at('2027-01-01T00:00:00+00:00')),
		).toBe(false);
	});

	it('never asks about a listing already resolved either way', () => {
		expect(hasRunOut(listing({ status: 'sold' }), at('2026-08-09T09:00:00+00:00'))).toBe(false);
		expect(hasRunOut(listing({ status: 'expired' }), at('2026-08-09T09:00:00+00:00'))).toBe(false);
	});

	it('counts only the listings actually waiting on an answer', () => {
		const rows = [
			listing({ id: 'a' }),
			listing({ id: 'b', expiresAt: '2026-09-01T18:20:00+00:00' }),
			listing({ id: 'c', status: 'sold' }),
			listing({ id: 'd', expiresAt: null, auctionDays: null }),
		];
		expect(runOutCount(rows, at('2026-08-09T09:00:00+00:00'))).toBe(1);
	});
});
