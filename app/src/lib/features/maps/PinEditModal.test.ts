// @vitest-environment happy-dom

import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeAll, describe, expect, it, vi } from 'vitest';
import type { PinConfig } from '$lib/api';
import PinEditModal from './PinEditModal.svelte';

const mocks = vi.hoisted(() => ({ getPinConfigs: vi.fn() }));
vi.mock('$lib/api', () => ({
	getPinConfigs: (...args: unknown[]) => mocks.getPinConfigs(...args),
}));

function treeConfig(): PinConfig {
	return {
		id: 9,
		planet: 'Arkadia',
		mapViewId: null,
		label: 'Tree',
		category: 'special',
		specialKind: 'tree',
		icon: '🌳',
		radiusM: null,
		colour: '#22c55e',
		cooldownColour: '#f59e0b',
		ordinal: 0,
		createdAt: 1,
		placedCount: 0,
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

describe('PinEditModal', () => {
	it('derives the persisted category from the single marker choice', async () => {
		mocks.getPinConfigs.mockResolvedValue([]);
		const onsubmit = vi.fn().mockResolvedValue(true);
		render(PinEditModal, {
			props: {
				open: true,
				point: { lon: 61_400, lat: 75_800 },
				planet: 'Calypso',
				mapViewId: null,
				editing: {
					id: 1,
					planet: 'Calypso',
					lon: 61_400,
					lat: 75_800,
					altitude: null,
					name: 'North claim',
					icon: 'ore',
					kind: 'legacy-custom-kind',
					radiusM: null,
					notes: null,
					sessionId: null,
					mapViewId: null,
					createdAt: 1,
					lastVisitedAt: null,
					cooldownUntil: null,
					pinConfigId: null,
					colour: null,
					cooldownColour: null,
					category: null,
					specialKind: null,
				},
				onsubmit,
			},
		});

		expect(screen.queryByText('Category')).toBeNull();
		expect(screen.getByRole('option', { name: '10 m area' })).toBeTruthy();
		await fireEvent.click(screen.getByRole('button', { name: 'Save' }));

		expect(onsubmit).toHaveBeenCalledWith({
			name: 'North claim',
			icon: 'ore',
			kind: 'mining',
			radiusM: null,
			notes: '',
		});
	});

	it('drops a pin from the chosen palette configuration', async () => {
		mocks.getPinConfigs.mockResolvedValue([treeConfig()]);
		const onsubmit = vi.fn().mockResolvedValue(true);
		render(PinEditModal, {
			props: {
				open: true,
				point: { lon: 100, lat: 200 },
				planet: 'Arkadia',
				mapViewId: null,
				editing: null,
				onsubmit,
			},
		});

		await waitFor(() => expect(mocks.getPinConfigs).toHaveBeenCalledWith('Arkadia', null));
		await screen.findByText(/Tree/);
		await fireEvent.click(screen.getByRole('button', { name: 'Drop pin' }));

		expect(onsubmit).toHaveBeenCalledWith({
			name: 'Tree',
			icon: '🌳',
			kind: 'tree',
			radiusM: null,
			notes: '',
			configId: 9,
		});
	});
});
