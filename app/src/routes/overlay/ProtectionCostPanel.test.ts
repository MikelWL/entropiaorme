// @vitest-environment happy-dom

import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { ProtectionCostStep } from '$lib/features/protection/protectionCostFlow';

const api = vi.hoisted(() => ({
	assignSessionProtectionLoadout: vi.fn(),
	confirmProtectionRepair: vi.fn(),
	scanRepairCost: vi.fn(),
	scanTradeTerminalValue: vi.fn(),
	confirmProtectionObservation: vi.fn(),
}));

vi.mock('$lib/api', () => api);

import ProtectionCostPanel from './ProtectionCostPanel.svelte';

const mixedSteps: ProtectionCostStep[] = [
	{
		layer: 'armour',
		method: 'repair',
		name: 'UL armour',
		setId: '1',
		armourSetId: '1',
		plateSetId: null,
		markupPercent: null,
		baselineTtPed: null,
	},
	{
		layer: 'plates',
		method: 'limited',
		name: 'L plates',
		setId: '2',
		armourSetId: null,
		plateSetId: '2',
		markupPercent: 125,
		baselineTtPed: null,
	},
];

beforeEach(() => {
	vi.clearAllMocks();
	api.confirmProtectionRepair.mockResolvedValue({
		costWindow: {
			id: 'window',
			kind: 'repair',
			setId: null,
			armourSetId: '1',
			plateSetId: null,
			consumedTtPed: null,
			markupPercent: null,
			costPed: 1.5,
			costKnown: true,
			status: 'booked',
			reason: null,
			createdAt: 1,
			allocations: [{ sessionId: 's1', hitCount: 1, allocationShare: 1, costPed: 1.5 }],
		},
	});
	api.scanTradeTerminalValue.mockResolvedValue({
		valuePed: 10,
		rawText: '10.00',
		confidence: 0.99,
		error: null,
		calibrated: true,
	});
	api.confirmProtectionObservation.mockResolvedValue({
		observation: {
			id: 'observation',
			setId: '2',
			ttValuePed: 10,
			source: 'ocr',
			rawText: '10.00',
			observedAt: 1,
			resetReason: null,
		},
		reconciliation: null,
		costWindow: null,
	});
});

describe('protection cost panel', () => {
	it('lets the user defer recording without consuming defensive evidence', async () => {
		const onClose = vi.fn();
		render(ProtectionCostPanel, {
			props: { sessionId: 's1', repairOcrEnabled: false, steps: mixedSteps, onClose },
		});

		await fireEvent.click(screen.getByText('Later'));
		expect(onClose).toHaveBeenCalledTimes(1);
		expect(api.confirmProtectionRepair).not.toHaveBeenCalled();
		expect(api.confirmProtectionObservation).not.toHaveBeenCalled();
	});

	it('records mixed protection armour-first and then establishes the limited plate baseline', async () => {
		const onClose = vi.fn();
		render(ProtectionCostPanel, {
			props: {
				sessionId: 's1',
				repairOcrEnabled: false,
				steps: mixedSteps,
				onClose,
			},
		});

		expect(screen.getByText('Armour')).toBeTruthy();
		expect(screen.getByText('Unlimited')).toBeTruthy();
		await fireEvent.click(screen.getByText('Enter manually'));
		await fireEvent.input(screen.getByPlaceholderText('0.00 PED'), {
			target: { value: '1.50' },
		});
		await fireEvent.click(screen.getByText('Confirm'));

		await waitFor(() =>
			expect(api.confirmProtectionRepair).toHaveBeenCalledWith(
				expect.objectContaining({ armourSetId: 1, plateSetId: null, costPed: 1.5 }),
			),
		);
		await fireEvent.click(screen.getByText('Continue to plates'));

		expect(screen.getByText('Plates')).toBeTruthy();
		expect(screen.getByText('Limited')).toBeTruthy();
		await fireEvent.click(screen.getByText('Scan Trade Terminal'));
		await waitFor(() => expect(screen.getByText('Set baseline')).toBeTruthy());
		await fireEvent.click(screen.getByText('Set baseline'));

		await waitFor(() =>
			expect(api.confirmProtectionObservation).toHaveBeenCalledWith(
				expect.objectContaining({ setId: 2, ttValuePed: 10, source: 'ocr' }),
			),
		);
		expect(screen.getByText('Baseline established')).toBeTruthy();
		await fireEvent.click(screen.getByText('Done'));
		expect(onClose).toHaveBeenCalledTimes(1);
	});

	it('captures a whole-session setup on Later without recording a cost', async () => {
		const onClose = vi.fn();
		api.assignSessionProtectionLoadout.mockResolvedValue({});
		render(ProtectionCostPanel, {
			props: {
				sessionId: 's1',
				repairOcrEnabled: false,
				steps: [],
				requiresLoadoutSelection: true,
				recordNow: false,
				protection: {
					sets: [],
					loadouts: [
						{
							id: '10',
							name: 'Jaguar and plates',
							armour: { id: '1', name: 'Jaguar', economyKind: 'unlimited', markupPercent: null },
							plates: null,
						},
					],
					activeLoadoutId: null,
					recentReconciliations: [],
					recentCostWindows: [],
				},
				onClose,
			},
		});

		await fireEvent.change(screen.getByRole('combobox'), { target: { value: '10' } });
		await waitFor(() =>
			expect(api.assignSessionProtectionLoadout).toHaveBeenCalledWith('s1', '10'),
		);
		expect(screen.getByText('Protection setup saved')).toBeTruthy();
		expect(api.confirmProtectionRepair).not.toHaveBeenCalled();
		await fireEvent.click(screen.getByText('Done'));
		expect(onClose).toHaveBeenCalledTimes(1);
	});

	it('accepts a lower limited-armour reading without an implementation assertion', async () => {
		render(ProtectionCostPanel, {
			props: {
				sessionId: 's1',
				repairOcrEnabled: false,
				steps: [{ ...mixedSteps[1], baselineTtPed: 10 }],
				onClose: vi.fn(),
			},
		});

		await fireEvent.click(screen.getByText('Enter manually'));
		await fireEvent.input(screen.getByPlaceholderText('0.00 PED'), { target: { value: '9' } });
		expect(screen.queryByText(/This set was not replaced/)).toBeNull();
		await fireEvent.click(screen.getByText('Confirm'));
		await waitFor(() =>
			expect(api.confirmProtectionObservation).toHaveBeenCalledWith(
				expect.objectContaining({ setId: 2, ttValuePed: 9 }),
			),
		);
	});
});
