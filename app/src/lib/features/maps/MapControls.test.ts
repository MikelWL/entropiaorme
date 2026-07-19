// @vitest-environment happy-dom

import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import MapControls from './MapControls.svelte';

describe('MapControls', () => {
	it('keeps the overlay primary and reveals pin search through setup', async () => {
		const ontoggleoverlay = vi.fn();
		const onconfigure = vi.fn();
		render(MapControls, {
			props: {
				pins: [],
				ontoggleoverlay,
				onconfigure,
				oncalibrate: vi.fn(),
				onroute: vi.fn(),
				onradarcalibrate: vi.fn(),
				onselectpin: vi.fn(),
			},
		});

		expect(screen.queryByRole('button', { name: 'Pin my location' })).toBeNull();
		expect(screen.queryByRole('combobox', { name: 'Search pins' })).toBeNull();
		await fireEvent.click(screen.getByRole('button', { name: 'Pin overlay' }));
		expect(ontoggleoverlay).toHaveBeenCalledOnce();

		await fireEvent.click(screen.getByRole('button', { name: 'Setup' }));
		await fireEvent.click(screen.getByRole('menuitem', { name: 'Search pins' }));
		expect(screen.getByRole('combobox', { name: 'Search pins' })).toBeTruthy();

		await fireEvent.click(screen.getByRole('button', { name: 'Setup' }));
		await fireEvent.click(screen.getByRole('menuitem', { name: 'Configure pin overlay' }));
		expect(onconfigure).toHaveBeenCalledOnce();
	});
});
