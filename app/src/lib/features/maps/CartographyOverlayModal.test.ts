// @vitest-environment happy-dom

import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import type { PinConfig } from '$lib/api';
import CartographyOverlayModal from './CartographyOverlayModal.svelte';

const mocks = vi.hoisted(() => ({
	getPinConfigs: vi.fn(),
	createPinConfig: vi.fn(),
	updatePinConfig: vi.fn(),
	deletePinConfig: vi.fn(),
	reorderPinConfigs: vi.fn(),
}));

vi.mock('$lib/api', () => ({
	getPinConfigs: (...args: unknown[]) => mocks.getPinConfigs(...args),
	createPinConfig: (...args: unknown[]) => mocks.createPinConfig(...args),
	updatePinConfig: (...args: unknown[]) => mocks.updatePinConfig(...args),
	deletePinConfig: (...args: unknown[]) => mocks.deletePinConfig(...args),
	reorderPinConfigs: (...args: unknown[]) => mocks.reorderPinConfigs(...args),
}));

function config(overrides: Partial<PinConfig> = {}): PinConfig {
	return {
		id: 5,
		planet: 'Arkadia',
		mapViewId: null,
		label: 'Claim',
		category: 'generic',
		specialKind: null,
		icon: '📍',
		radiusM: null,
		colour: '#38bdf8',
		cooldownColour: null,
		ordinal: 0,
		createdAt: 1,
		placedCount: 3,
		...overrides,
	};
}

beforeAll(() => {
	Element.prototype.animate = function animate() {
		const animation = {
			cancel() {},
			finish() {},
			effect: null,
			currentTime: 0,
			playState: 'finished',
			onfinish: null as (() => void) | null,
			oncancel: null as (() => void) | null,
		};
		queueMicrotask(() => animation.onfinish?.());
		return animation as unknown as Animation;
	};
});

beforeEach(() => {
	mocks.getPinConfigs.mockReset().mockResolvedValue([config()]);
	mocks.createPinConfig.mockReset().mockResolvedValue(config({ id: 10, category: 'special', specialKind: 'tree', label: 'Tree' }));
	mocks.updatePinConfig.mockReset().mockResolvedValue(config());
	mocks.deletePinConfig.mockReset().mockResolvedValue(undefined);
	mocks.reorderPinConfigs.mockReset().mockResolvedValue(undefined);
});

describe('CartographyOverlayModal', () => {
	it('adds a tree option and saves the palette to the database', async () => {
		const onchanged = vi.fn();
		render(CartographyOverlayModal, {
			props: { open: true, planet: 'Arkadia', mapViewId: null, mapName: 'Default', onchanged },
		});

		await waitFor(() => expect(mocks.getPinConfigs).toHaveBeenCalledWith('Arkadia', null));
		await screen.findByDisplayValue('Claim');

		await fireEvent.click(screen.getByRole('button', { name: 'Add tree' }));
		await fireEvent.click(screen.getByRole('button', { name: 'Save' }));

		await waitFor(() => expect(mocks.createPinConfig).toHaveBeenCalledTimes(1));
		expect(mocks.updatePinConfig).toHaveBeenCalledWith(5, expect.objectContaining({ category: 'generic' }));
		expect(mocks.createPinConfig).toHaveBeenCalledWith(
			expect.objectContaining({
				planet: 'Arkadia',
				mapViewId: null,
				category: 'special',
				specialKind: 'tree',
			}),
		);
		expect(mocks.reorderPinConfigs).toHaveBeenCalledWith([5, 10]);
		expect(onchanged).toHaveBeenCalled();
	});

	it('confirms a cascade delete naming the placed-pin count', async () => {
		const onchanged = vi.fn();
		render(CartographyOverlayModal, {
			props: { open: true, planet: 'Arkadia', mapViewId: null, mapName: 'Default', onchanged },
		});
		await screen.findByDisplayValue('Claim');

		await fireEvent.click(screen.getByRole('button', { name: 'Remove Claim' }));
		expect(screen.getByText(/deletes the 3 pins already placed/i)).toBeTruthy();

		await fireEvent.click(screen.getByRole('button', { name: 'Remove' }));
		await waitFor(() => expect(mocks.deletePinConfig).toHaveBeenCalledWith(5));
		expect(onchanged).toHaveBeenCalled();
	});
});
