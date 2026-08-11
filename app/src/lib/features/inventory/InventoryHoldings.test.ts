// @vitest-environment happy-dom

import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import { marketOpportunity } from '$lib/features/analytics/treeCuttingModel.svelte';
import InventoryHoldings from './InventoryHoldings.svelte';

describe('inventory assets', () => {
	it('uses the shared inventory layout and compact actions for assets', async () => {
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
				oncreatelisting: vi.fn(),
				oneditequipment: onedit,
				onsellequipment: onsell,
				ondeleteequipment: onremove,
			},
		});

		const utility = screen.getByTestId('equipment-utility-strip');
		expect(within(utility).getByLabelText('Find an item')).not.toBeNull();
		expect(within(utility).getByRole('button', { name: 'Add asset' })).not.toBeNull();

		const table = screen.getByRole('table', { name: 'Assets' });
		expect(
			within(table)
				.getAllByRole('columnheader')
				.map((header) => header.textContent),
		).toEqual(['Asset', 'TT', 'MU paid', 'Total cost', 'Acquired', 'Actions']);
		expect(screen.getByText('Ares Ring, Improved')).not.toBeNull();
		const assetCell = screen.getByText('Ares Ring, Improved').parentElement;
		const actionCell = screen.getByRole('button', { name: 'Edit' }).parentElement;
		expect(assetCell?.classList.contains('flex-1')).toBe(true);
		expect(actionCell?.classList.contains('min-w-[5.25rem]')).toBe(true);
		expect(actionCell?.classList.contains('w-[5.25rem]')).toBe(false);

		await fireEvent.click(screen.getByRole('button', { name: 'Edit' }));
		await fireEvent.click(screen.getByRole('button', { name: 'Sell' }));
		await fireEvent.click(screen.getByRole('button', { name: 'Remove' }));

		expect(onedit).toHaveBeenCalledWith(equipment);
		expect(onsell).toHaveBeenCalledWith(equipment);
		expect(onremove).toHaveBeenCalledWith(equipment);
	});

	it('lets loot actions expand by yielding space from the item column', () => {
		render(InventoryHoldings, {
			props: {
				kind: 'loot',
				onkindchange: vi.fn(),
				confidenceMode: 'liquid',
				onconfidencechange: vi.fn(),
				loot: [
					{
						itemName: 'Animal Muscle Oil',
						heldQty: 100,
						heldTt: 1,
						listedQty: 0,
						readings: [],
						opportunity: marketOpportunity(undefined, 100.6),
						markupPct: null,
						markupHorizon: null,
						tier: 'illiquid',
						effectiveMarkupPct: 100.6,
						floored: true,
						salesPed: null,
						weeklySalesPed: null,
					},
				],
				equipment: [],
				onsellloot: vi.fn(),
				onconvert: vi.fn(),
				onremove: vi.fn(),
				onshrapnel: vi.fn(),
				onaddequipment: vi.fn(),
				oncreatelisting: vi.fn(),
				oneditequipment: vi.fn(),
				onsellequipment: vi.fn(),
				ondeleteequipment: vi.fn(),
			},
		});

		const actionCell = screen.getByRole('button', { name: 'Sell' }).parentElement;
		expect(actionCell?.classList.contains('min-w-[5.25rem]')).toBe(true);
		expect(actionCell?.classList.contains('w-[5.25rem]')).toBe(false);
	});
});
