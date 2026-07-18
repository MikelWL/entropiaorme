// @vitest-environment happy-dom

import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import type { MapView } from '$lib/api';
import MapViewSelector from './MapViewSelector.svelte';

const created: MapView = {
	id: 7,
	planet: 'Calypso',
	name: 'New map',
	createdAt: 1_752_000_000,
};

describe('MapViewSelector', () => {
	it('creates a map and selects its generated name for immediate replacement', async () => {
		const onadd = vi.fn().mockResolvedValue(created);
		const rendered = render(MapViewSelector, {
			views: [],
			selectedId: null,
			onselect: vi.fn(),
			onadd,
			onrename: vi.fn().mockResolvedValue(true),
			ondelete: vi.fn().mockResolvedValue(true),
		});

		await fireEvent.click(screen.getByRole('button', { name: 'Default' }));
		fireEvent.click(screen.getByRole('menuitem', { name: 'Add map' }));
		await vi.waitFor(() => expect(onadd).toHaveBeenCalledOnce());
		await rendered.rerender({
			views: [created],
			selectedId: 7,
			onselect: vi.fn(),
			onadd,
			onrename: vi.fn().mockResolvedValue(true),
			ondelete: vi.fn().mockResolvedValue(true),
		});

		const input = (await screen.findByRole('textbox', { name: 'Map name' })) as HTMLInputElement;
		expect(input.value).toBe('New map');
		expect(document.activeElement).toBe(input);
		expect(input.selectionStart).toBe(0);
		expect(input.selectionEnd).toBe('New map'.length);
	});
});
