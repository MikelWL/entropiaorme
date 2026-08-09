// @vitest-environment happy-dom

import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import { createTableModel, type TableModel } from '$lib/view/tableModel.svelte';
import HuntingPrimaryView from './HuntingPrimaryView.svelte';
import HuntingSessions from './HuntingSessions.svelte';
import type {
	HuntingActivitySection,
	HuntingOverallLine,
	HuntingSessionSection,
} from './huntingModel.svelte';
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
		activities: [],
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

function overall(overrides: Partial<HuntingOverallLine> = {}): HuntingOverallLine {
	return {
		cycled: 180,
		returns: 162,
		lootRate: 0.9,
		muProjectedReturns: 190.8,
		muRate: 1.06,
		realisedMarkup: 27,
		realisedReturns: 189,
		realisedRate: 1.05,
		realisedOutsidePeriod: 0,
		...overrides,
	};
}

function primaryProps(
	table: TableModel<HuntingSessionSection>,
	selected: HuntingSessionSection | null,
	onselect = vi.fn(),
) {
	return {
		overall: overall(),
		stock: [],
		table,
		selected,
		totalCount: table.filtered.length,
		onselect,
		onsell: vi.fn(),
		onconvert: vi.fn(),
	};
}

function activity(overrides: Partial<HuntingActivitySection> = {}): HuntingActivitySection {
	return {
		kind: 'quest',
		label: 'Daily Hunting 1',
		cycled: 100,
		returns: 90,
		lootRate: 0.9,
		confirmedRewardPed: 0,
		rewardedReturns: 90,
		rewardedRate: 0.9,
		rewardStatus: 'none',
		lootItems: [],
		key: 'quest:daily-hunting-1',
		isUnscoped: false,
		muProjectedReturns: 106,
		muRewardedReturns: 106,
		muRewardedRate: 1.06,
		items: [item],
		variants: [],
		...overrides,
	};
}

describe('Hunting economic comparisons', () => {
	it('uses the Tree Cutting frame for sessions and omits legacy activity statistics', async () => {
		const row = session();
		const table = createTableModel<HuntingSessionSection>({
			rows: () => [row],
			pageSize: Number.MAX_SAFE_INTEGER,
		});
		render(HuntingPrimaryView, { props: primaryProps(table, row) });

		for (const label of ['TT Net', 'MU Net', 'Realised Net']) {
			expect(screen.getByText(label)).not.toBeNull();
		}
		expect(screen.getByText('Animal Muscle Oil')).not.toBeNull();
		const trigger = screen.getByLabelText('Switch hunting view (currently ARIS Dailies)');
		expect(trigger.className).not.toContain('border');
		expect(screen.getByTitle('ARIS Dailies').className).toContain('text-text');
		expect(screen.getByTestId('activity-economic-headline').className).toContain(
			'grid-cols-[minmax(10rem,1.35fr)_repeat(3,minmax(0,1fr))]',
		);
		expect(screen.getByTestId('economic-subordinate-cycled').textContent).toContain('100.00');
		const ttRate = within(screen.getByTestId('economic-subordinate-tt-rate')).getByText('90.0%');
		const muRate = within(screen.getByTestId('economic-subordinate-mu-rate')).getByText('106.0%');
		const realisedRate = within(screen.getByTestId('economic-subordinate-realised-rate')).getByText(
			'105.0%',
		);
		expect(ttRate.classList.contains('text-text')).toBe(true);
		expect(ttRate.classList.contains('text-negative')).toBe(false);
		expect(muRate.classList.contains('text-text')).toBe(true);
		expect(muRate.classList.contains('text-positive')).toBe(false);
		expect(realisedRate.classList.contains('text-positive')).toBe(true);
		expect(screen.queryByRole('menu')).toBeNull();
		await fireEvent.click(trigger);
		const menu = screen.getByRole('menu');
		expect(within(menu).getByRole('menuitem', { name: /Overall/ })).not.toBeNull();
		for (const label of ['Cycled', 'MU Rate', 'Realised Rate']) {
			expect(within(menu).getByText(label)).not.toBeNull();
		}
		expect(screen.queryByRole('button', { name: 'Activities' })).toBeNull();
		expect(screen.queryByRole('button', { name: 'Loot' })).toBeNull();
		for (const legacy of ['PES/100', 'Kills', 'Runs', 'Instances', 'Duration', 'Net / Kill']) {
			expect(screen.queryByText(legacy)).toBeNull();
		}
	});

	it('pins unassigned sessions last in the picker and suppresses their economic metrics', async () => {
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
		render(HuntingPrimaryView, { props: primaryProps(table, unassigned) });

		await fireEvent.click(screen.getByLabelText('Switch hunting view (currently Unassigned)'));
		const rows = screen.getAllByRole('menuitem');
		expect(rows).toHaveLength(3);
		expect(rows[0].textContent).toContain('Overall');
		expect(rows[1].textContent).toContain('ARIS Dailies');
		expect(rows[2].textContent).toContain('Unassigned');
		expect(rows[2].textContent).not.toContain('999.00');
		expect(screen.queryByText('TT Net')).toBeNull();
	});

	it('starts on Overall and returns there from the session picker', async () => {
		const row = session();
		const table = createTableModel<HuntingSessionSection>({
			rows: () => [row],
			pageSize: Number.MAX_SAFE_INTEGER,
		});
		const onselect = vi.fn();
		const view = render(HuntingPrimaryView, { props: primaryProps(table, null, onselect) });

		expect(screen.getByLabelText('Switch hunting view (currently Overall)')).not.toBeNull();
		expect(screen.getByText('180.00')).not.toBeNull();
		await fireEvent.click(screen.getByLabelText('Switch hunting view (currently Overall)'));
		await fireEvent.click(screen.getByRole('menuitem', { name: /ARIS Dailies/ }));
		expect(onselect).toHaveBeenCalledWith('definition:7');

		await view.rerender(primaryProps(table, row, onselect));
		await fireEvent.click(screen.getByLabelText('Switch hunting view (currently ARIS Dailies)'));
		await fireEvent.click(screen.getByRole('menuitem', { name: /Overall/ }));
		expect(onselect).toHaveBeenCalledWith(null);
	});

	it('offers Activities and Loot only when the session has declared activities', () => {
		const row = session({ activities: [activity()] });
		render(HuntingSessions, {
			props: { selected: row },
		});

		expect(screen.getByRole('button', { name: 'Activities' })).not.toBeNull();
		expect(screen.getByRole('button', { name: 'Loot' })).not.toBeNull();
		expect(screen.getByText('Daily Hunting 1')).not.toBeNull();
	});

	it('opens Loot directly when the only activity evidence is Unscoped', () => {
		const row = session({
			activities: [activity({ kind: 'ambient', label: 'Unscoped', isUnscoped: true })],
		});
		render(HuntingSessions, {
			props: { selected: row },
		});

		expect(screen.queryByRole('button', { name: 'Activities' })).toBeNull();
		expect(screen.queryByRole('button', { name: 'Loot' })).toBeNull();
		expect(screen.queryByText('Unscoped')).toBeNull();
		expect(screen.getByText('Animal Muscle Oil')).not.toBeNull();
	});

	it('filters rich economic rows and selects from the keyboard-ready overlay', async () => {
		const current = session();
		const alternative = session({
			definitionId: 8,
			name: 'General Hunting',
			key: 'definition:8',
			cycled: 80,
			realisedRate: 0.98,
		});
		const table = createTableModel<HuntingSessionSection>({
			rows: () => [current, alternative],
			pageSize: Number.MAX_SAFE_INTEGER,
			searchText: (row) => [row.name],
			initialSort: { key: 'cycled', dir: 'desc' },
		});
		const onselect = vi.fn();
		render(HuntingPrimaryView, { props: primaryProps(table, current, onselect) });

		const trigger = screen.getByLabelText('Switch hunting view (currently ARIS Dailies)');
		await fireEvent.keyDown(trigger, { key: 'ArrowDown' });
		const filter = screen.getByLabelText('Filter analytics sessions');
		expect(document.activeElement).toBe(filter);
		const menu = screen.getByRole('menu');
		expect(within(menu).getByText('100.00')).not.toBeNull();
		expect(within(menu).getAllByText('105.0%')).toHaveLength(2);

		await fireEvent.input(filter, { target: { value: 'general' } });
		const match = screen.getByRole('menuitem', { name: /General Hunting/ });
		expect(screen.queryByRole('menuitem', { name: /ARIS Dailies/ })).toBeNull();
		await fireEvent.keyDown(filter, { key: 'ArrowDown' });
		expect(document.activeElement).toBe(match);
		await fireEvent.click(match);

		expect(onselect).toHaveBeenCalledWith('definition:8');
		expect(screen.queryByRole('menu')).toBeNull();
	});
});
