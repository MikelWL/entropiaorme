// @vitest-environment happy-dom

import { render, screen, within } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import { NO_DATA } from '$lib/utils/format';
import TreeCuttingActivities from './TreeCuttingActivities.svelte';
import type { TreeCuttingSection } from './treeCuttingModel.svelte';

function section(
	yieldTier: TreeCuttingSection['yieldTier'],
	overrides: Partial<TreeCuttingSection> = {},
): TreeCuttingSection {
	return {
		yieldTier,
		swings: 2,
		cycled: 0.4,
		returns: 0.3,
		lootRate: 0.75,
		muProjectedReturns: 0.45,
		muRate: 1.125,
		realisedReturns: 0.3,
		realisedRate: 0.75,
		realisedMarkup: 0,
		items: [],
		tools: [],
		...overrides,
	};
}

describe('TreeCuttingActivities', () => {
	// The detail pane reports the same three nets as Overall. A markup-only
	// figure here broke that pairing and left no row answering "what did this
	// activity actually make".
	it('reports Realised Net against cycled, matching its TT Net sibling', () => {
		const huge = section('huge', {
			cycled: 10,
			returns: 8,
			realisedReturns: 12,
			realisedMarkup: 4,
		});
		render(TreeCuttingActivities, {
			props: {
				sections: [huge],
				selected: huge,
				onselect: vi.fn(),
				sortKey: 'cycled',
				sortDir: 'desc',
				onsort: vi.fn(),
			},
		});

		expect(screen.getByText('Realised Net')).not.toBeNull();
		expect(screen.queryByText('Realised MU')).toBeNull();
		// TT Net is 8 - 10; Realised Net folds in the 4 of confirmed markup and
		// is measured against cycled, not against TT Net.
		expect(screen.getByText('-2.00')).not.toBeNull();
		expect(screen.getByText('+2.00')).not.toBeNull();
	});

	it('presents Unclassified as a diagnostic without activity economics', () => {
		const unclassified = section('unknown', {
			swings: 4,
			cycled: 99,
			muRate: 9.99,
			realisedRate: 8.88,
		});
		render(TreeCuttingActivities, {
			props: {
				sections: [unclassified, section('huge')],
				selected: unclassified,
				onselect: vi.fn(),
				sortKey: 'cycled',
				sortDir: 'desc',
				onsort: vi.fn(),
			},
		});

		const activityRows = screen.getAllByRole('listitem');
		expect(activityRows).toHaveLength(2);
		expect(activityRows[0].textContent).toContain('Long Boards');
		expect(activityRows[1].textContent).toContain('Unclassified');

		const unclassifiedButton = within(activityRows[1]).getByRole('button');
		expect(unclassifiedButton.textContent).toContain('Activity metrics not applicable');
		expect(unclassifiedButton.textContent).not.toContain('99.00');
		expect(unclassifiedButton.textContent).not.toContain('999.0%');
		expect(unclassifiedButton.textContent).not.toContain('888.0%');

		expect(
			screen.getByText(/4 swings are unclassified and cannot be assigned to a board activity/),
		).not.toBeNull();
		expect(screen.queryByText('TT Net')).toBeNull();
		expect(screen.getByRole('button', { name: 'Why swings can be unclassified' })).not.toBeNull();
		expect(screen.getByRole('tooltip').textContent).toContain(
			'Its recorded cost and loot still count in Overall',
		);
	});

	it('lists tool strategies inside a classified activity in payload order', () => {
		// The panel renders `selected.tools` as given: the backend emits them by
		// descending cost, so a test that sorted them here would hide a
		// regression in that ordering rather than catch it.
		const huge = section('huge', {
			tools: [
				{
					key: 'ph-4',
					toolName: 'Terratech PH-4 (L)',
					swings: 4558,
					cycled: 90.84,
					returns: 33.96,
					lootRate: 0.3738,
					muProjectedReturns: null,
					muRate: null,
					realisedReturns: 33.96,
					realisedRate: 0.3738,
				},
				{
					key: 'ph-3',
					toolName: 'Terratech PH-3',
					swings: 4,
					cycled: 0.4,
					returns: 0.3,
					lootRate: 0.75,
					muProjectedReturns: 0.45,
					muRate: 1.125,
					realisedReturns: 0.3,
					realisedRate: 0.75,
				},
			],
		});
		render(TreeCuttingActivities, {
			props: {
				sections: [huge],
				selected: huge,
				onselect: vi.fn(),
				sortKey: 'cycled',
				sortDir: 'desc',
				onsort: vi.fn(),
			},
		});

		expect(screen.getByText('Tool strategy')).not.toBeNull();
		const names = screen.getAllByText(/Terratech PH-[34]/).map((node) => node.textContent?.trim());
		expect(names).toEqual(['Terratech PH-4 (L)', 'Terratech PH-3']);
		// A tool with no market evidence reads as no data, never as a zero rate.
		// Scoped to the tool table, since the detail pane's MU Net reads as no
		// data on the same fixture for the same reason.
		const toolRows = screen.getAllByText(/Terratech PH-[34]/).map((n) => n.closest('li'));
		const ph4Row = toolRows[0];
		expect(ph4Row).not.toBeNull();
		expect(ph4Row?.textContent).toContain(NO_DATA);
		expect(ph4Row?.textContent).not.toContain('0.0%');
	});
});
