// @vitest-environment happy-dom

import { render, screen, within } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import { createTableModel } from '$lib/view/tableModel.svelte';
import HuntingSessions from './HuntingSessions.svelte';
import HuntingTargets from './HuntingTargets.svelte';
import type { HuntingSessionSection, HuntingTargetSection } from './huntingModel.svelte';
import { marketOpportunity } from './treeCuttingModel.svelte';

const item = {
	name: 'Animal Muscle Oil',
	quantity: 40,
	ttValue: 12,
	sharePct: 100,
	opportunity: marketOpportunity(undefined, 100.6),
	ownMarkupPct: 130,
	markupHorizon: 'week',
	effectiveMarkupPct: 130,
	floored: false,
	tier: 'liquid' as const,
	salesPed: 5000,
	weeklySalesPed: 5000,
};

function session(overrides: Partial<HuntingSessionSection> = {}): HuntingSessionSection {
	return {
		definitionId: 7,
		name: 'ARIS Dailies',
		isArchived: false,
		cycled: 100,
		returns: 90,
		lootRate: 0.9,
		lootItems: [],
		key: 'definition:7',
		isUnassigned: false,
		realisedMarkup: 15,
		muProjectedReturns: 106,
		muRate: 1.06,
		realisedReturns: 105,
		realisedRate: 1.05,
		items: [item],
		...overrides,
	};
}

function target(overrides: Partial<HuntingTargetSection> = {}): HuntingTargetSection {
	return {
		mobSpecies: 'Atrox',
		cycled: 100,
		returns: 90,
		lootRate: 0.9,
		lootItems: [],
		key: 'species:Atrox',
		label: 'Atrox',
		isUnclassified: false,
		realisedMarkup: 15,
		muProjectedReturns: 106,
		muRate: 1.06,
		realisedReturns: 105,
		realisedRate: 1.05,
		items: [item],
		...overrides,
	};
}

describe('Hunting economic comparisons', () => {
	it('uses the Tree Cutting frame for sessions and omits legacy activity statistics', () => {
		const row = session();
		const table = createTableModel<HuntingSessionSection>({
			rows: () => [row],
			pageSize: Number.MAX_SAFE_INTEGER,
		});
		render(HuntingSessions, {
			props: { table, selected: row, onselect: vi.fn() },
		});

		for (const label of [
			'Cycled',
			'MU Rate',
			'Realised Rate',
			'TT Net',
			'MU Net',
			'Realised Net',
		]) {
			expect(screen.getByText(label)).not.toBeNull();
		}
		expect(screen.getByText('Animal Muscle Oil')).not.toBeNull();
		for (const legacy of ['PES/100', 'Kills', 'Runs', 'Instances', 'Duration', 'Net / Kill']) {
			expect(screen.queryByText(legacy)).toBeNull();
		}
	});

	it('pins unassigned sessions last and suppresses their economic metrics', () => {
		const unassigned = session({
			definitionId: null,
			name: 'Unassigned',
			key: 'unassigned',
			isUnassigned: true,
			cycled: 999,
		});
		const defined = session();
		const table = createTableModel<HuntingSessionSection>({
			rows: () => [unassigned, defined],
			pageSize: Number.MAX_SAFE_INTEGER,
		});
		render(HuntingSessions, {
			props: { table, selected: unassigned, onselect: vi.fn() },
		});

		const rows = screen.getAllByRole('listitem');
		expect(rows).toHaveLength(2);
		expect(rows[0].textContent).toContain('ARIS Dailies');
		expect(rows[1].textContent).toContain('Unassigned');
		expect(within(rows[1]).getByRole('button').textContent).not.toContain('999.00');
		expect(screen.queryByText('TT Net')).toBeNull();
	});

	it('uses the same compact economic detail for species without maturity or kill drilldowns', () => {
		const row = target();
		const table = createTableModel<HuntingTargetSection>({
			rows: () => [row],
			pageSize: Number.MAX_SAFE_INTEGER,
		});
		render(HuntingTargets, {
			props: { table, selected: row, onselect: vi.fn() },
		});

		expect(screen.getByText('TT Net')).not.toBeNull();
		expect(screen.getByText('Animal Muscle Oil')).not.toBeNull();
		for (const legacy of ['PES/100', 'Kills', 'Maturity', 'Net / Kill']) {
			expect(screen.queryByText(legacy)).toBeNull();
		}
	});
});
