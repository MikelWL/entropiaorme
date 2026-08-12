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

/** Whether an open listing has passed the moment it was posted through to,
 * given the current time in epoch milliseconds.
 *
 * False whenever the duration was never recorded: no duration, no deadline,
 * so nothing to ask about.
 *
 * The comparison is between instants, not between date strings. The expiry
 * arrives as a UTC timestamp while a calendar date here would be the local
 * one, and comparing across those two frames asks the question early for any
 * listing whose local deadline falls after midnight UTC: half an hour early
 * an hour east of UTC, most of a day further east. It would also throw away
 * the time of day the deadline is recorded to carry. */
export function hasRunOut(listing: AuctionListing, now: number): boolean {
	if (listing.status !== 'pending') return false;
	if (!listing.expiresAt) return false;
	const expiry = Date.parse(listing.expiresAt);
	// An unreadable stamp is not a deadline that has passed; say nothing
	// rather than raise a question about a listing that may well be live.
	if (Number.isNaN(expiry)) return false;
	return expiry <= now;
}

/** How many open listings have run their course and are waiting to be told
 * what became of them. */
export function runOutCount(listings: AuctionListing[], now: number): number {
	return listings.filter((listing) => hasRunOut(listing, now)).length;
}
