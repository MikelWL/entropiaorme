import { describe, expect, it } from 'vitest';
import {
	capturedDraft,
	derivedTt,
	draftIssues,
	EMPTY_DRAFT,
	impliedMarkupPct,
	isCommittable,
	type ListingDraftFields,
	previewNetMarkup,
} from './listingIntake';

/** An auction draft: 10 PED TT listed at 10.25 / 11.50, which the surface
 * shows as 102.50% / 115.00%. */
function auctionDraft(overrides: Partial<ListingDraftFields> = {}): ListingDraftFields {
	return {
		...EMPTY_DRAFT,
		itemName: 'Long Moonleaf Board',
		quantity: 5,
		ttValue: 10,
		auctionFee: 0.5,
		auctionDays: 7,
		startingBid: 10.25,
		buyout: 11.5,
		...overrides,
	};
}

describe('listing intake', () => {
	it('accepts a complete auction draft', () => {
		expect(draftIssues(auctionDraft(), 'auction')).toEqual([]);
		expect(isCommittable(auctionDraft(), 'auction')).toBe(true);
	});

	it('reports a below-TT starting bid without blocking it', () => {
		const draft = auctionDraft({ startingBid: 9, buyout: null });
		const issues = draftIssues(draft, 'auction');
		expect(issues).toHaveLength(1);
		expect(issues[0]).toMatchObject({ severity: 'advisory' });
		expect(isCommittable(draft, 'auction')).toBe(true);
	});

	it('blocks a buyout beneath the starting bid', () => {
		const draft = auctionDraft({ buyout: 9 });
		expect(draftIssues(draft, 'auction')).toContainEqual(
			expect.objectContaining({ field: 'buyout', severity: 'blocking' }),
		);
	});

	it('requires the basics of any sale', () => {
		const issues = draftIssues(EMPTY_DRAFT, 'auction');
		expect(issues.map((issue) => issue.field)).toEqual(
			expect.arrayContaining(['itemName', 'quantity', 'startingBid']),
		);
	});

	it('requires a whole number of days when a duration is given', () => {
		expect(draftIssues(auctionDraft({ auctionDays: 2.5 }), 'auction')).toContainEqual(
			expect.objectContaining({ field: 'auctionDays', severity: 'blocking' }),
		);
		expect(draftIssues(auctionDraft({ auctionDays: 0 }), 'auction')).toContainEqual(
			expect.objectContaining({ field: 'auctionDays', severity: 'blocking' }),
		);
		expect(draftIssues(auctionDraft({ auctionDays: null }), 'auction')).toEqual([]);
	});

	it('asks a trade only for its price', () => {
		const draft: ListingDraftFields = {
			...EMPTY_DRAFT,
			itemName: 'Long Moonleaf Board',
			quantity: 5,
			ttValue: 10,
			buyout: 11,
		};
		expect(isCommittable(draft, 'trade')).toBe(true);
		// No fee, bid, or duration is demanded of a trade.
		expect(draftIssues(draft, 'trade')).toEqual([]);
	});

	it('derives markup only against a usable TT', () => {
		expect(impliedMarkupPct(10, 0)).toBeNull();
		expect(impliedMarkupPct(null, 10)).toBeNull();
		expect(impliedMarkupPct(10.25, 10)).toBeCloseTo(102.5, 6);
	});

	it('works TT out from the quantity and what the stock was recorded at', () => {
		const shrapnel = {
			kind: 'loot',
			holdingId: 'Shrapnel',
			name: 'Shrapnel',
			score: 100,
			unitTt: 0.0001,
			heldQty: 120_000,
		};
		expect(derivedTt(shrapnel, 50_000)).toBeCloseTo(5, 6);
		// Rounded to the hundredth the game displays, not carried out to the
		// full precision of a per-unit value that was itself a division.
		expect(derivedTt({ ...shrapnel, unitTt: 1 / 3 }, 10)).toBe(3.33);
		// Nothing to work from is not zero: zero would read as a free sale.
		expect(derivedTt({ ...shrapnel, unitTt: null }, 10)).toBeNull();
		expect(derivedTt(shrapnel, null)).toBeNull();
		expect(derivedTt(shrapnel, 0)).toBeNull();
		expect(derivedTt(null, 10)).toBeNull();
	});

	it('previews the net a sale would leave after the fee already spent', () => {
		// Buyout 11.50 on 10.00 of TT, less the 0.50 fee.
		expect(previewNetMarkup(auctionDraft(), 'auction')).toBeCloseTo(1, 6);
		// A trade pays no fee.
		expect(previewNetMarkup(auctionDraft({ buyout: 11.5 }), 'trade')).toBeCloseTo(1.5, 6);
		expect(previewNetMarkup(auctionDraft({ ttValue: null }), 'auction')).toBeNull();
	});
});

describe('capturedDraft', () => {
	const read = {
		observedName: 'Shrapnel',
		quantity: 2754889,
		ttValue: 275.48,
		listingFee: 64.51,
		auctionDays: 6,
		startingBid: 9276,
		buyout: 9276,
	};

	it('fills the draft from what the window said', () => {
		expect(capturedDraft(read)).toEqual({
			itemName: 'Shrapnel',
			quantity: 2754889,
			ttValue: 275.48,
			auctionFee: 64.51,
			auctionDays: 6,
			startingBid: 9276,
			buyout: 9276,
		});
	});

	it('leaves a field the read refused empty rather than guessing at it', () => {
		const draft = capturedDraft({ ...read, quantity: null, startingBid: null });
		expect(draft.quantity).toBeNull();
		expect(draft.startingBid).toBeNull();
		// A refusal must not be reported as committable: an empty quantity is
		// exactly what the manual path already blocks on.
		expect(isCommittable(draft, 'auction')).toBe(false);
	});

	it('treats an unread name as no name at all', () => {
		expect(capturedDraft({ ...read, observedName: null }).itemName).toBe('');
	});

	it('carries a captured draft through the same checks a typed one meets', () => {
		expect(isCommittable(capturedDraft(read), 'auction')).toBe(true);
	});

	it('derives a markup off the shown TT, which the window has rounded', () => {
		// The window showed TT 275.48 and markup 3367.10%. Those disagree:
		// 9276 / 275.48 is 3367.21%. The game computes against the unrounded
		// TT, which for 2,754,889 shrapnel at 0.0001 PED is 275.4889, and
		// only the display is cut to two decimals.
		expect(impliedMarkupPct(read.startingBid, read.ttValue)).toBeCloseTo(3367.21, 2);
		expect(impliedMarkupPct(read.startingBid, 275.4889)).toBeCloseTo(3367.1, 2);
		// So a derived percentage can differ from the game's in its last
		// decimal. It is a display figure either way; nothing accounts on it.
	});
});
