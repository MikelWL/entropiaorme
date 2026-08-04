import { describe, expect, it, vi } from 'vitest';
import { createSessionFacets, type SessionFacetsDeps } from './sessionFacets.svelte';

function harness(overrides: Partial<SessionFacetsDeps> = {}) {
	const facetState = {
		name: null as string | null,
		definitionId: null as string | null,
		boost: null as number | null,
	};
	const setSessionConfig = vi.fn(async (name: string | null, boost: number | null) => {
		facetState.name = name;
		facetState.boost = boost;
	});
	// Mirrors the backend verb: selecting writes the definition AND the
	// name facet together.
	const selectDefinition = vi.fn(async (id: number) => {
		facetState.definitionId = String(id);
		facetState.name = `Definition ${id}`;
	});
	const deps: SessionFacetsDeps = {
		readFacets: () => ({ ...facetState }),
		isSessionActive: () => true,
		refresh: vi.fn(async () => {}),
		setSessionConfig,
		selectDefinition,
		...overrides,
	};
	return {
		facets: createSessionFacets(deps),
		deps,
		facetState,
		setSessionConfig,
		selectDefinition,
	};
}

describe('session facet', () => {
	it('shapes the selection write (numeric id) and refreshes', async () => {
		const { facets, facetState, selectDefinition, deps } = harness();

		await facets.selectDefinition('4');

		expect(selectDefinition).toHaveBeenCalledWith(4);
		expect(deps.refresh).toHaveBeenCalled();
		expect(facetState.definitionId).toBe('4');
		expect(facetState.name).toBe('Definition 4');
	});

	it('surfaces a selection refusal instead of swallowing it', async () => {
		const { facets } = harness({
			selectDefinition: vi.fn(async () => {
				throw new Error('backend said no');
			}),
		});

		await facets.selectDefinition('4');

		expect(facets.facetError).toBe('backend said no');
		expect(facets.savingDefinition).toBe(false);
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
