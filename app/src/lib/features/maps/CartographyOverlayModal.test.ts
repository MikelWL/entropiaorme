// @vitest-environment happy-dom

import { fireEvent, render, screen } from '@testing-library/svelte';
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
	it('reorders compact marker definitions and derives their categories on save', async () => {
		mocks.setCartographyOverlayConfig.mockClear();
		render(CartographyOverlayModal, {
			props: {
				open: true,
				config: {
					planet: 'Calypso',
					buttons: [
						{ id: 'ore', name: 'Claim', icon: 'ore', kind: 'custom', radiusM: null },
						{ id: 'home', name: 'Camp', icon: 'home', kind: 'custom', radiusM: 50 },
					],
				},
			},
		});

		expect(screen.queryByText('Category')).toBeNull();
		await fireEvent.click(screen.getByRole('button', { name: 'Move Claim down' }));
		await fireEvent.click(screen.getByRole('button', { name: 'Save' }));

		expect(mocks.setCartographyOverlayConfig).toHaveBeenCalledWith({
			planet: 'Calypso',
			buttons: [
				{ id: 'home', name: 'Camp', icon: 'home', kind: 'location', radiusM: 50 },
				{ id: 'ore', name: 'Claim', icon: 'ore', kind: 'mining', radiusM: null },
			],
		});
	});
});
