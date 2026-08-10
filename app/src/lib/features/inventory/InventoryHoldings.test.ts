// @vitest-environment happy-dom

import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import InventoryHoldings from './InventoryHoldings.svelte';

describe('inventory holdings', () => {
	it('uses the shared holdings layout and compact actions for equipment', async () => {
		const onedit = vi.fn();
		const onsell = vi.fn();
		const onremove = vi.fn();
		const equipment = {
			id: 'equipment-1',
			name: 'Ares Ring, Improved',
			ttValue: 25,
			markupPaid: 75,
			notes: 'Tier 4',
			acquiredAt: '2026-08-01',
		};

		render(InventoryHoldings, {
			props: {
				kind: 'equipment',
				onkindchange: vi.fn(),
				confidenceMode: 'liquid',
				onconfidencechange: vi.fn(),
				loot: [],
				equipment: [equipment],
				onsellloot: vi.fn(),
				onconvert: vi.fn(),
				onremove: vi.fn(),
				onshrapnel: vi.fn(),
				onaddequipment: vi.fn(),
				oneditequipment: onedit,
				onsellequipment: onsell,
				ondeleteequipment: onremove,
			},
		});

		const utility = screen.getByTestId('equipment-utility-strip');
		expect(within(utility).getByLabelText('Find an item')).not.toBeNull();
		expect(within(utility).getByRole('button', { name: 'Add holding' })).not.toBeNull();

		const table = screen.getByRole('table', { name: 'Equipment holdings' });
		expect(
			within(table)
				.getAllByRole('columnheader')
				.map((header) => header.textContent),
		).toEqual(['Holding', 'TT', 'Markup paid', 'Cost basis', 'Acquired', 'Actions']);
		expect(screen.getByText('Ares Ring, Improved')).not.toBeNull();

		await fireEvent.click(screen.getByRole('button', { name: 'Edit' }));
		await fireEvent.click(screen.getByRole('button', { name: 'Sell' }));
		await fireEvent.click(screen.getByRole('button', { name: 'Remove' }));

		expect(onedit).toHaveBeenCalledWith(equipment);
		expect(onsell).toHaveBeenCalledWith(equipment);
		expect(onremove).toHaveBeenCalledWith(equipment);
	});
});
