import { describe, expect, it, vi } from 'vitest';
import { createSessionFacets, type SessionFacetsDeps } from './sessionFacets.svelte';

function harness(overrides: Partial<SessionFacetsDeps> = {}) {
	const facetState = {
		name: null as string | null,
		boost: null as number | null,
		segment: null as string | null,
	};
	const setSessionConfig = vi.fn(async (name: string | null, boost: number | null) => {
		facetState.name = name;
		facetState.boost = boost;
	});
	// Mirrors the backend's auto-numbering: a null name is numbered by
	// open count, every open replaces the standing segment, and the
	// acknowledgement echoes the applied name.
	let segmentsOpened = 0;
	const openSegment = vi.fn(async (name: string | null) => {
		segmentsOpened += 1;
		facetState.segment = name ?? `Segment ${segmentsOpened}`;
		return { segmentName: facetState.segment };
	});
	const closeSegment = vi.fn(async () => {
		facetState.segment = null;
	});
	const renameSegment = vi.fn(async (name: string) => {
		facetState.segment = name;
	});
	const focusQuest = vi.fn(async () => {});
	const unfocusQuest = vi.fn(async () => {});
	const deps: SessionFacetsDeps = {
		readFacets: () => ({ ...facetState }),
		isSessionActive: () => true,
		refresh: vi.fn(async () => {}),
		searchNames: vi.fn(async () => []),
		setSessionConfig,
		openSegment,
		closeSegment,
		renameSegment,
		focusQuest,
		unfocusQuest,
		openNameMenu: vi.fn(),
		closeNameMenu: vi.fn(),
		...overrides,
	};
	return {
		facets: createSessionFacets(deps),
		deps,
		facetState,
		setSessionConfig,
		openSegment,
		closeSegment,
		renameSegment,
		focusQuest,
		unfocusQuest,
	};
}

describe('name facet', () => {
	it('writes the name and carries the boost through untouched', async () => {
		const { facets, facetState, setSessionConfig } = harness();
		facetState.boost = 50;

		await facets.applyName('ARIS Dailies');

		expect(setSessionConfig).toHaveBeenCalledWith('ARIS Dailies', 50);
		expect(facetState).toEqual({ name: 'ARIS Dailies', boost: 50, segment: null });
	});

	it('clears the name without disturbing the boost', async () => {
		const { facets, facetState, setSessionConfig } = harness();
		facetState.name = 'ARIS Dailies';
		facetState.boost = 50;

		await facets.clearName();

		expect(setSessionConfig).toHaveBeenCalledWith(null, 50);
		expect(facetState.boost).toBe(50);
	});

	it('ignores an empty name rather than writing a blank one', async () => {
		const { facets, setSessionConfig } = harness();
		await facets.applyName('   '.trim());
		expect(setSessionConfig).not.toHaveBeenCalled();
	});

	it('surfaces a write failure instead of swallowing it', async () => {
		const { facets } = harness({
			setSessionConfig: vi.fn(async () => {
				throw new Error('backend said no');
			}),
		});

		await facets.applyName('ARIS Dailies');

		expect(facets.facetError).toBe('backend said no');
		expect(facets.savingName).toBe(false);
	});
});

describe('boost facet', () => {
	it('parses the draft and writes it beside the standing name', async () => {
		const { facets, facetState, setSessionConfig } = harness();
		facetState.name = 'ARIS Dailies';
		facets.boostDraft = '50';

		await facets.commitBoost();

		expect(setSessionConfig).toHaveBeenCalledWith('ARIS Dailies', 50);
		expect(facets.boostDraft).toBe('50');
	});

	it('treats an empty draft as withdrawing the declaration', async () => {
		const { facets, facetState, setSessionConfig } = harness();
		facetState.boost = 50;
		facets.boostDraft = '';

		await facets.commitBoost();

		expect(setSessionConfig).toHaveBeenCalledWith(null, null);
	});

	// The three-state distinction the segment model rests on: a typed 0 is
	// a real declaration ("deliberately unboosted"), which is the baseline
	// a boost's effect is measured against. An empty field claims nothing.
	// Collapsing the two would erase the only baseline the app can record.
	it('writes a typed zero as a declaration, not as a withdrawal', async () => {
		const { facets, setSessionConfig } = harness();
		facets.boostDraft = '0';

		await facets.commitBoost();

		expect(setSessionConfig).toHaveBeenCalledWith(null, 0);
		expect(facets.boostDraft).toBe('0');
	});

	it('distinguishes clearing the field from declaring zero', async () => {
		const { facets, facetState, setSessionConfig } = harness();
		facetState.boost = 0;
		facets.boostDraft = '';

		await facets.commitBoost();

		// Declared-zero -> withdrawn is a real move, so it must write.
		expect(setSessionConfig).toHaveBeenCalledWith(null, null);
	});

	it('does not rewrite a declared zero that has not moved', async () => {
		const { facets, facetState, setSessionConfig } = harness();
		facetState.boost = 0;
		facets.boostDraft = '0';

		await facets.commitBoost();

		expect(setSessionConfig).not.toHaveBeenCalled();
		expect(facets.boostDraft).toBe('0');
	});

	it('withdraws rather than inventing a magnitude for a negative draft', async () => {
		const { facets, facetState, setSessionConfig } = harness();
		facetState.boost = 50;
		facets.boostDraft = '-10';

		await facets.commitBoost();

		expect(setSessionConfig).toHaveBeenCalledWith(null, null);
		expect(facets.boostDraft).toBe('');
	});

	it('renders a persisted zero as "0" rather than an empty field', () => {
		const { facets, facetState } = harness();
		facetState.boost = 0;

		facets.syncBoostDraft();

		expect(facets.boostDraft).toBe('0');
	});

	it('normalises an unparseable draft without writing', async () => {
		const { facets, setSessionConfig } = harness();
		facets.boostDraft = 'abc';

		await facets.commitBoost();

		// Nothing moved (no boost either way), so no write; the buffer must
		// not keep showing text that was never persisted.
		expect(setSessionConfig).not.toHaveBeenCalled();
		expect(facets.boostDraft).toBe('');
	});

	it('does not rewrite an unchanged value', async () => {
		const { facets, facetState, setSessionConfig } = harness();
		facetState.boost = 50;
		facets.boostDraft = ' 50 ';

		await facets.commitBoost();

		expect(setSessionConfig).not.toHaveBeenCalled();
		expect(facets.boostDraft).toBe('50');
	});

	it('syncs the draft from the persisted value', () => {
		const { facets, facetState } = harness();
		facetState.boost = 100;

		facets.syncBoostDraft();

		expect(facets.boostDraft).toBe('100');
	});
});

describe('segment facet', () => {
	it('opens with the typed draft as the name', async () => {
		const { facets, facetState, openSegment } = harness();
		facets.segmentDraft = '  Boss: Kreltin  ';

		await facets.commitSegment();

		expect(openSegment).toHaveBeenCalledWith('Boss: Kreltin');
		expect(facetState.segment).toBe('Boss: Kreltin');
	});

	it('opens with a null name when the draft is blank, leaving the auto-number to the backend', async () => {
		const { facets, facetState, openSegment } = harness();

		await facets.commitSegment();

		expect(openSegment).toHaveBeenCalledWith(null);
		expect(facetState.segment).toBe('Segment 1');
	});

	it('renames the open segment instead of opening a second one', async () => {
		const { facets, facetState, openSegment, renameSegment } = harness();
		facetState.segment = 'Segment 1';
		facets.segmentDraft = 'Boss 1';

		await facets.commitSegment();

		expect(renameSegment).toHaveBeenCalledWith('Boss 1');
		expect(openSegment).not.toHaveBeenCalled();
		expect(facetState.segment).toBe('Boss 1');
	});

	it('normalises a blank or unchanged rename without writing', async () => {
		const { facets, facetState, renameSegment } = harness();
		facetState.segment = 'Boss 1';

		facets.segmentDraft = '   ';
		await facets.commitSegment();
		expect(facets.segmentDraft).toBe('Boss 1');

		facets.segmentDraft = 'Boss 1';
		await facets.commitSegment();
		expect(renameSegment).not.toHaveBeenCalled();
	});

	it('always auto-numbers the next segment even while the draft shows the current name', async () => {
		const { facets, facetState, openSegment } = harness();
		await facets.commitSegment();
		facets.segmentDraft = facetState.segment ?? '';

		await facets.nextSegment();

		expect(openSegment).toHaveBeenLastCalledWith(null);
		expect(facetState.segment).toBe('Segment 2');
	});

	it('honours a typed draft on the next-segment click when none is open', async () => {
		const { facets, openSegment } = harness();
		facets.segmentDraft = 'Boss 1';

		await facets.nextSegment();

		expect(openSegment).toHaveBeenCalledWith('Boss 1');
	});

	it('clears the draft on close', async () => {
		const { facets, facetState, closeSegment } = harness();
		await facets.commitSegment();
		facets.segmentDraft = 'Segment 1';

		await facets.closeSegment();

		expect(closeSegment).toHaveBeenCalled();
		expect(facetState.segment).toBeNull();
		expect(facets.segmentDraft).toBe('');
	});

	it('commits only a rename on blur: clicking away never opens a segment', async () => {
		const { facets, facetState, openSegment, renameSegment } = harness();

		facets.segmentDraft = 'Prospective';
		await facets.handleSegmentBlur();
		expect(openSegment).not.toHaveBeenCalled();
		expect(facets.segmentDraft).toBe('Prospective');

		facetState.segment = 'Segment 1';
		facets.segmentDraft = 'Boss 1';
		await facets.handleSegmentBlur();
		expect(renameSegment).toHaveBeenCalledWith('Boss 1');
	});

	it('keeps the buffer in step with the persisted name, except while no segment is open', () => {
		const { facets, facetState } = harness();
		facetState.segment = 'Boss 2';
		facets.syncSegmentDraft();
		expect(facets.segmentDraft).toBe('Boss 2');

		facetState.segment = null;
		facets.syncSegmentDraft();
		expect(facets.segmentDraft).toBe('');
	});

	it('surfaces a write failure instead of swallowing it', async () => {
		const { facets } = harness({
			openSegment: vi.fn(async () => {
				throw new Error('backend said no');
			}),
		});

		await facets.commitSegment();

		expect(facets.facetError).toBe('backend said no');
		expect(facets.savingSegment).toBe(false);
	});

	// The write's own snapshot refresh re-runs the host's draft-sync
	// effect while savingSegment still guards it, so that sync is a
	// no-op by design; the buffer must come from the write's result, or
	// the field stays stale until the next unrelated snapshot frame
	// (the original symptom: the name only appeared on the next kill).
	it('shows the applied name immediately even though the refresh-time sync is guarded', async () => {
		const { facets, deps } = harness();
		// What the route's effect does when the refresh lands.
		deps.refresh = vi.fn(async () => {
			facets.syncSegmentDraft();
		});

		await facets.nextSegment();
		expect(facets.segmentDraft).toBe('Segment 1');

		await facets.nextSegment();
		expect(facets.segmentDraft).toBe('Segment 2');

		facets.segmentDraft = 'Boss 2';
		await facets.commitSegment();
		expect(facets.segmentDraft).toBe('Boss 2');
	});
});
