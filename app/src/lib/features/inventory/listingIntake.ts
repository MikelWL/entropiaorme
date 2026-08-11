/**
 * The listing-intake draft: what the game's sale window says, before any of
 * it becomes a transaction.
 *
 * The window shows eight things, and three of them are not independent: the
 * markup percentages are the bids over the TT value. That redundancy is the
 * point. A typed digit and a misread digit both show up as the stated markup
 * disagreeing with the one the bids imply, which is a far stronger check than
 * any per-field plausibility rule could be, and it costs the user nothing when
 * the numbers are right.
 *
 * So the markup fields are optional, but not decorative: supplying them and
 * ignoring what they say would be worse than not supplying them at all. When
 * they disagree, intake stops. The player fixes the digit, or clears the
 * markup field and proceeds knowingly.
 *
 * This module is deliberately pure and intake-neutral. A typed form fills the
 * draft today; the screen-capture adapter fills the same shape with the same
 * checks, so a misread cannot reach the ledger by a path a typo could not.
 */

/** What the sale window states, as read or typed. Percentages are the game's
 * own display form: 102.5 means 102.50%. */
export interface ListingDraftFields {
	itemName: string;
	quantity: number | null;
	ttValue: number | null;
	auctionFee: number | null;
	markupSbPct: number | null;
	markupBoPct: number | null;
	auctionDays: number | null;
	startingBid: number | null;
	buyout: number | null;
}

export interface DraftIssue {
	/** Which field the reader should look at first. */
	field: keyof ListingDraftFields;
	/** `blocking` stops the commit; `advisory` is worth saying and no more. */
	severity: 'blocking' | 'advisory';
	message: string;
}

export const EMPTY_DRAFT: ListingDraftFields = {
	itemName: '',
	quantity: null,
	ttValue: null,
	auctionFee: null,
	markupSbPct: null,
	markupBoPct: null,
	auctionDays: null,
	startingBid: null,
	buyout: null,
};

/** Half of the last displayed decimal place: every figure in the window is
 * shown rounded to hundredths, so this is the most any of them can be out. */
const DISPLAY_HALF_ULP = 0.005;

/**
 * How far the stated markup may sit from the one the bid and TT imply before
 * the two are genuinely inconsistent rather than merely rounded differently.
 *
 * Both inputs to `bid / tt` are displayed to hundredths, so the quotient
 * inherits both errors, and the stated percentage is itself rounded. Deriving
 * the tolerance from that propagation rather than picking a round number keeps
 * it honest across the whole TT range: a 0.1pp window is generous at 100 PED
 * TT and far too tight at 1 PED, where the same rounding is worth a full point.
 */
export function markupTolerancePct(ttValue: number, markupPct: number): number {
	const fromBid = (100 * DISPLAY_HALF_ULP) / ttValue;
	const fromTt = (markupPct * DISPLAY_HALF_ULP) / ttValue;
	return fromBid + fromTt + DISPLAY_HALF_ULP;
}

/** The markup a bid represents against a TT value, in the game's percentage
 * form. `null` when TT is missing or zero, where the ratio says nothing. */
export function impliedMarkupPct(bid: number | null, ttValue: number | null): number | null {
	if (bid === null || ttValue === null || ttValue <= 0) return null;
	return (bid / ttValue) * 100;
}

function markupIssue(
	field: 'markupSbPct' | 'markupBoPct',
	label: string,
	stated: number | null,
	bid: number | null,
	ttValue: number | null,
): DraftIssue | null {
	const implied = impliedMarkupPct(bid, ttValue);
	if (stated === null || implied === null || ttValue === null) return null;
	if (Math.abs(implied - stated) <= markupTolerancePct(ttValue, stated)) return null;
	return {
		field,
		severity: 'blocking',
		message:
			`The ${label} works out at ${implied.toFixed(2)}% of TT, but the window is ` +
			`recorded as saying ${stated.toFixed(2)}%. One of the three figures is wrong.`,
	};
}

/**
 * Everything wrong with a draft, worst first. An empty list means it can be
 * committed; any `blocking` entry means it cannot.
 */
export function draftIssues(draft: ListingDraftFields, channel: 'auction' | 'trade'): DraftIssue[] {
	const issues: DraftIssue[] = [];

	if (draft.itemName.trim() === '') {
		issues.push({ field: 'itemName', severity: 'blocking', message: 'Name the item being sold.' });
	}
	if (draft.quantity === null || draft.quantity <= 0) {
		issues.push({
			field: 'quantity',
			severity: 'blocking',
			message: 'Quantity must be above zero.',
		});
	}
	if (draft.ttValue !== null && draft.ttValue < 0) {
		issues.push({
			field: 'ttValue',
			severity: 'blocking',
			message: 'TT value cannot be negative.',
		});
	}

	if (channel === 'auction') {
		if (draft.startingBid === null || draft.startingBid <= 0) {
			issues.push({
				field: 'startingBid',
				severity: 'blocking',
				message: 'An auction needs a starting bid.',
			});
		}
		if (draft.auctionFee !== null && draft.auctionFee < 0) {
			issues.push({
				field: 'auctionFee',
				severity: 'blocking',
				message: 'A fee cannot be negative.',
			});
		}
		if (
			draft.auctionDays !== null &&
			(draft.auctionDays <= 0 || !Number.isInteger(draft.auctionDays))
		) {
			issues.push({
				field: 'auctionDays',
				severity: 'blocking',
				message: 'A listing runs for a whole number of days.',
			});
		}
		// A buyout under the starting bid is not a misreading we can resolve
		// for the player, and the game would not have accepted it, so it is
		// reported as the contradiction it is.
		if (draft.buyout !== null && draft.startingBid !== null && draft.buyout < draft.startingBid) {
			issues.push({
				field: 'buyout',
				severity: 'blocking',
				message: 'The buyout is below the starting bid.',
			});
		}

		const sb = markupIssue(
			'markupSbPct',
			'starting bid',
			draft.markupSbPct,
			draft.startingBid,
			draft.ttValue,
		);
		if (sb) issues.push(sb);
		const bo = markupIssue('markupBoPct', 'buyout', draft.markupBoPct, draft.buyout, draft.ttValue);
		if (bo) issues.push(bo);

		// Not an error: selling at or under TT is a real (if grim) choice, and
		// the recycler is often the better one. Worth saying once, not blocking.
		const impliedSb = impliedMarkupPct(draft.startingBid, draft.ttValue);
		if (impliedSb !== null && impliedSb < 100) {
			issues.push({
				field: 'startingBid',
				severity: 'advisory',
				message: 'The starting bid is below TT, so a sale at that price loses value.',
			});
		}
	} else if (draft.buyout === null || draft.buyout <= 0) {
		// A trade has one price and no fee, bid, or duration.
		issues.push({
			field: 'buyout',
			severity: 'blocking',
			message: 'A trade needs the price it sold for.',
		});
	}

	return issues.sort((a, b) =>
		a.severity === b.severity ? 0 : a.severity === 'blocking' ? -1 : 1,
	);
}

/** Whether the draft may cross the commit boundary. */
export function isCommittable(draft: ListingDraftFields, channel: 'auction' | 'trade'): boolean {
	return !draftIssues(draft, channel).some((issue) => issue.severity === 'blocking');
}

/** The net a sale at this price would leave, after the fee already spent. Not
 * a realised figure: nothing is realised until an auction actually closes. */
export function previewNetMarkup(
	draft: ListingDraftFields,
	channel: 'auction' | 'trade',
): number | null {
	const price = channel === 'trade' ? draft.buyout : (draft.buyout ?? draft.startingBid);
	if (price === null || draft.ttValue === null) return null;
	const fee = channel === 'auction' ? (draft.auctionFee ?? 0) : 0;
	return price - draft.ttValue - fee;
}
