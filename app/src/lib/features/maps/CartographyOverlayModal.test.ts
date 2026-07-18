// @vitest-environment happy-dom

import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { beforeAll, describe, expect, it, vi } from 'vitest';
import CartographyOverlayModal from './CartographyOverlayModal.svelte';

const mocks = vi.hoisted(() => ({
	setCartographyOverlayConfig: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('./cartographyOverlay.svelte', async (importOriginal) => {
	const actual = await importOriginal<typeof import('./cartographyOverlay.svelte')>();
	return { ...actual, setCartographyOverlayConfig: mocks.setCartographyOverlayConfig };
});

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

describe('CartographyOverlayModal', () => {
	it('searches for an emoji and saves neutral marker definitions', async () => {
		mocks.setCartographyOverlayConfig.mockClear();
		render(CartographyOverlayModal, {
			props: {
				open: true,
				config: {
					planet: 'Calypso',
					mapViewId: null,
					buttons: [
						{ id: 'ore', name: 'Claim', icon: 'ore', kind: 'custom', radiusM: null },
						{ id: 'home', name: 'Camp', icon: 'home', kind: 'custom', radiusM: 50 },
					],
				},
			},
		});

		expect(screen.queryByText('Category')).toBeNull();
		expect(screen.queryByText('Ore claim')).toBeNull();
		expect(screen.getAllByRole('option', { name: '10 m area' })).toHaveLength(2);
		expect(screen.getByRole('button', { name: 'Remove Claim' })).toBeTruthy();
		await fireEvent.click(screen.getByRole('button', { name: 'Choose emoji for Claim' }));
		const picker = within(screen.getAllByRole('dialog', { name: 'Choose emoji' })[0]);
		expect(
			picker.getAllByRole('button').map((option) => option.getAttribute('aria-label')),
		).toEqual([
			'round pushpin',
			'triangular flag',
			'star',
			'pick',
			'alien monster',
			'deciduous tree',
			'cyclone',
			'house',
			'droplet',
		]);
		await fireEvent.input(picker.getByRole('textbox', { name: 'Search emoji' }), {
			target: { value: 'dragon' },
		});
		await fireEvent.click(picker.getByRole('button', { name: 'dragon' }));
		await fireEvent.click(screen.getByRole('button', { name: 'Move Claim down' }));
		await fireEvent.click(screen.getByRole('button', { name: 'Save' }));

		expect(mocks.setCartographyOverlayConfig).toHaveBeenCalledWith({
			planet: 'Calypso',
			mapViewId: null,
			buttons: [
				{ id: 'home', name: 'Camp', icon: '🏠', kind: 'marker', radiusM: 50 },
				{ id: 'ore', name: 'Claim', icon: '🐉', kind: 'marker', radiusM: null },
			],
		});
	});
});
