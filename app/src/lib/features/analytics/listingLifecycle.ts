/**
 * When an open listing's clock has run out.
 *
 * A listing that has passed its duration has *not* been decided: in game it
 * either sold or came back, and the app has no way to know which. So this
 * says only that the question is now answerable, and never guesses at the
 * answer. Presuming it expired would write a false outcome into holdings and
 * the ledger; presuming it sold would invent realised markup out of nothing.
 *
 * Its whole job is to raise the question at the moment it becomes worth
 * asking, and then get out of the way.
 */
import type { AuctionListing } from '$lib/types/analytics';

/** Whether an open listing has passed the day it was posted through to.
 * False whenever the duration was never recorded: no duration, no deadline,
 * so nothing to ask about. */
export function hasRunOut(listing: AuctionListing, today: string): boolean {
	if (listing.status !== 'pending') return false;
	if (!listing.expiresAt) return false;
	// Plain ISO dates compare correctly as strings, which keeps this free of
	// any timezone question the comparison does not actually need.
	return listing.expiresAt < today;
}

/** How many open listings have run their course and are waiting to be told
 * what became of them. */
export function runOutCount(listings: AuctionListing[], today: string): number {
	return listings.filter((listing) => hasRunOut(listing, today)).length;
}
