// @vitest-environment happy-dom

import { render, screen, within } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
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
		items: [],
		tools: [],
		...overrides,
	};
}

describe('TreeCuttingActivities', () => {
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
});
