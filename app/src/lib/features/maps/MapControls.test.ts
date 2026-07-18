// @vitest-environment happy-dom

import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import type { PlanetMap } from '$lib/api';
import MapControls from './MapControls.svelte';

const planets: PlanetMap[] = [
	{
		name: 'Calypso',
		technicalName: 'Calypso',
		imageMime: 'image/jpeg',
		imageWidthPx: 100,
		imageHeightPx: 100,
		calibration: null,
	},
];

describe('MapControls', () => {
	it('exposes the primary, setup, search, coordinate, and grid actions', async () => {
		const onscan = vi.fn();
		const onsearchenter = vi.fn();
		const ongoto = vi.fn();
		const onconfigure = vi.fn();
		render(MapControls, {
			props: {
				planets,
				selectedName: 'Calypso',
				scanning: false,
				visiblePins: 1,
				totalPins: 3,
				onselectplanet: vi.fn(),
				onscan,
				ontoggleoverlay: vi.fn(),
				onconfigure,
				oncalibrate: vi.fn(),
				onsearchenter,
				ongoto,
			},
		});

		await fireEvent.click(screen.getByRole('button', { name: 'Pin my location' }));
		expect(onscan).toHaveBeenCalledOnce();

		await fireEvent.keyDown(screen.getByPlaceholderText('Name or notes'), { key: 'Enter' });
		expect(onsearchenter).toHaveBeenCalledOnce();

		await fireEvent.input(screen.getByPlaceholderText('61400, 75800'), {
			target: { value: '61400, 75800' },
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Go' }));
		expect(ongoto).toHaveBeenCalledOnce();

		const grid = screen.getByRole('button', { name: 'Grid' });
		expect(grid.getAttribute('aria-pressed')).toBe('false');
		await fireEvent.click(grid);
		expect(grid.getAttribute('aria-pressed')).toBe('true');

		await fireEvent.click(screen.getByRole('button', { name: 'Setup' }));
		await fireEvent.click(screen.getByRole('menuitem', { name: 'Configure pin overlay' }));
		expect(onconfigure).toHaveBeenCalledOnce();
		expect(screen.getByText('1/3')).toBeTruthy();
	});
});
