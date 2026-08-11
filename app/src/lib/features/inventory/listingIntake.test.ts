import { describe, expect, it } from 'vitest';
import {
	draftIssues,
	EMPTY_DRAFT,
	impliedMarkupPct,
	isCommittable,
	type ListingDraftFields,
	markupTolerancePct,
	previewNetMarkup,
} from './listingIntake';

/** A consistent auction draft: 10 PED TT listed at 102.50% / 115.00%. */
function auctionDraft(overrides: Partial<ListingDraftFields> = {}): ListingDraftFields {
	return {
		...EMPTY_DRAFT,
		itemName: 'Long Moonleaf Board',
		quantity: 5,
		ttValue: 10,
		auctionFee: 0.5,
		markupSbPct: 102.5,
		markupBoPct: 115,
		auctionDays: 7,
		startingBid: 10.25,
		buyout: 11.5,
		...overrides,
	};
}

describe('listing intake', () => {
	it('accepts a draft whose stated markups agree with its bids', () => {
		expect(draftIssues(auctionDraft(), 'auction')).toEqual([]);
		expect(isCommittable(auctionDraft(), 'auction')).toBe(true);
	});

	it('blocks a draft whose stated markup contradicts its bid', () => {
		// A transposed digit in the starting bid: 10.25 typed as 10.52, which
		// on its own looks like a perfectly ordinary price.
		const issues = draftIssues(auctionDraft({ startingBid: 10.52 }), 'auction');
		expect(issues[0]).toMatchObject({ field: 'markupSbPct', severity: 'blocking' });
		expect(issues[0].message).toContain('105.20%');
		expect(isCommittable(auctionDraft({ startingBid: 10.52 }), 'auction')).toBe(false);
	});

	it('tolerates the rounding the game itself displays', () => {
		// 10.25 / 10.00 is exactly 102.5%; a TT displayed as 10.00 could be
		// anything up to 10.005, which moves the true markup off the stated
		// figure without either number being wrong.
		const draft = auctionDraft({ ttValue: 10.004, startingBid: 10.25 });
		expect(draftIssues(draft, 'auction')).toEqual([]);
	});

	it('scales its tolerance with TT, because rounding hurts small values most', () => {
		// The same half-hundredth of rounding is worth ten times more markup
		// on a 1 PED item than on a 10 PED one.
		expect(markupTolerancePct(1, 100)).toBeGreaterThan(markupTolerancePct(10, 100) * 5);
	});

	it('leaves the markup check out entirely when no markup was recorded', () => {
		const draft = auctionDraft({ markupSbPct: null, markupBoPct: null, startingBid: 999 });
		// Nothing contradicts anything: an unstated markup makes no claim.
		expect(draftIssues(draft, 'auction').some((issue) => issue.field === 'markupSbPct')).toBe(
			false,
		);
	});

	it('reports a below-TT starting bid without blocking it', () => {
		const draft = auctionDraft({
			startingBid: 9,
			markupSbPct: 90,
			buyout: null,
			markupBoPct: null,
		});
		const issues = draftIssues(draft, 'auction');
		expect(issues).toHaveLength(1);
		expect(issues[0]).toMatchObject({ severity: 'advisory' });
		expect(isCommittable(draft, 'auction')).toBe(true);
	});

	it('blocks a buyout beneath the starting bid', () => {
		const draft = auctionDraft({ buyout: 9, markupBoPct: 90 });
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

	it('previews the net a sale would leave after the fee already spent', () => {
		// Buyout 11.50 on 10.00 of TT, less the 0.50 fee.
		expect(previewNetMarkup(auctionDraft(), 'auction')).toBeCloseTo(1, 6);
		// A trade pays no fee.
		expect(previewNetMarkup(auctionDraft({ buyout: 11.5 }), 'trade')).toBeCloseTo(1.5, 6);
		expect(previewNetMarkup(auctionDraft({ ttValue: null }), 'auction')).toBeNull();
	});
});
