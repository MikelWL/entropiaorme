// @vitest-environment happy-dom

import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { ProtectionCostStep } from '$lib/features/protection/protectionCostFlow';

const api = vi.hoisted(() => ({
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
			allocations: [
				{ sessionId: 's1', damageWeight: 10, deflectionCount: 0, allocationShare: 1, costPed: 1.5 },
			],
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
});
