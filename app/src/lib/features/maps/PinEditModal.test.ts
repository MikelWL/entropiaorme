// @vitest-environment happy-dom

import { fireEvent, render, screen } from '@testing-library/svelte';
import { beforeAll, describe, expect, it, vi } from 'vitest';
import PinEditModal from './PinEditModal.svelte';

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
		const onsubmit = vi.fn().mockResolvedValue(true);
		render(PinEditModal, {
			props: {
				open: true,
				point: { lon: 61_400, lat: 75_800 },
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
});
