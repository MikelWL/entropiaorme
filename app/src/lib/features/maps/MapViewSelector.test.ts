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

const second: MapView = {
	...created,
	id: 8,
	name: 'Mining',
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

	it('does not save a pending rename before a cancelled deletion', async () => {
		const onrename = vi.fn().mockResolvedValue(true);
		const ondelete = vi.fn().mockResolvedValue(true);
		vi.spyOn(window, 'confirm').mockReturnValue(false);
		render(MapViewSelector, {
			views: [created],
			selectedId: 7,
			onselect: vi.fn(),
			onadd: vi.fn().mockResolvedValue(null),
			onrename,
			ondelete,
		});

		await fireEvent.click(screen.getByRole('button', { name: 'New map' }));
		await fireEvent.click(screen.getByRole('menuitem', { name: 'Rename New map' }));
		const input = screen.getByRole('textbox', { name: 'Map name' });
		await fireEvent.input(input, { target: { value: 'Trees' } });
		const deleteButton = screen.getByRole('menuitem', { name: 'Delete New map' });
		await fireEvent.pointerDown(deleteButton);
		await fireEvent.blur(input);
		await fireEvent.click(deleteButton);

		expect(onrename).not.toHaveBeenCalled();
		expect(ondelete).not.toHaveBeenCalled();
		expect((input as HTMLInputElement).value).toBe('New map');
	});

	it('restores the edited row when deletion of another row is cancelled', async () => {
		const onrename = vi.fn().mockResolvedValue(true);
		vi.spyOn(window, 'confirm').mockReturnValue(false);
		render(MapViewSelector, {
			views: [created, second],
			selectedId: 7,
			onselect: vi.fn(),
			onadd: vi.fn().mockResolvedValue(null),
			onrename,
			ondelete: vi.fn().mockResolvedValue(true),
		});

		await fireEvent.click(screen.getByRole('button', { name: 'New map' }));
		await fireEvent.click(screen.getByRole('menuitem', { name: 'Rename New map' }));
		const input = screen.getByRole('textbox', { name: 'Map name' });
		await fireEvent.input(input, { target: { value: 'Trees' } });
		const deleteButton = screen.getByRole('menuitem', { name: 'Delete Mining' });
		await fireEvent.pointerDown(deleteButton);
		await fireEvent.blur(input);
		await fireEvent.click(deleteButton);

		expect(onrename).not.toHaveBeenCalled();
		expect((input as HTMLInputElement).value).toBe('New map');
	});
});
