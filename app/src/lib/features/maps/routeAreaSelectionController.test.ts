import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { MapsModel } from './mapsModel.svelte';

const mocked = vi.hoisted(() => ({
	handlers: new Map<string, (event: { payload: unknown }) => void>(),
	emit: vi.fn(),
	hide: vi.fn(),
	show: vi.fn(),
}));

vi.mock('@tauri-apps/api/event', () => ({
	emit: mocked.emit,
	listen: vi.fn((name: string, handler: (event: { payload: unknown }) => void) => {
		mocked.handlers.set(name, handler);
		return Promise.resolve(() => mocked.handlers.delete(name));
	}),
}));
vi.mock('$lib/api', () => ({
	hideNavigationOverlays: mocked.hide,
	showNavigationOverlays: mocked.show,
}));

import { createRouteAreaSelectionController } from './routeAreaSelectionController.svelte';

beforeEach(() => {
	vi.clearAllMocks();
	mocked.handlers.clear();
	mocked.hide.mockResolvedValue(undefined);
	mocked.show.mockResolvedValue(undefined);
});

function model(planet = 'Calypso', mapViewId: number | null = null) {
	return {
		selected: { name: planet },
		selectedViewId: mapViewId,
	} as unknown as MapsModel;
}

describe('route-area selection window coordination', () => {
	it('enters only for the current map context, then returns the exact selection', async () => {
		const controller = createRouteAreaSelectionController(model());
		const stop = controller.mount();
		await vi.waitFor(() =>
			expect(mocked.handlers.has('navigation-area-selection-requested')).toBe(true),
		);

		mocked.handlers.get('navigation-area-selection-requested')?.({
			payload: { requestId: 3, planet: 'Calypso', mapViewId: null },
		});
		await vi.waitFor(() => expect(mocked.hide).toHaveBeenCalledOnce());
		expect(controller.active).toBe(true);
		controller.setRegions([{ left: 1, top: 2, right: 3, bottom: 4 }]);
		await controller.confirm([8, 2]);

		expect(mocked.emit).toHaveBeenCalledWith('navigation-area-selection-result', {
			requestId: 3,
			planet: 'Calypso',
			mapViewId: null,
			pinIds: [2, 8],
		});
		expect(mocked.show).toHaveBeenCalledOnce();
		expect(controller.active).toBe(false);
		stop();
	});

	it('declines stale HUD context without hiding the overlay', async () => {
		const controller = createRouteAreaSelectionController(model('Calypso', 4));
		const stop = controller.mount();
		await vi.waitFor(() =>
			expect(mocked.handlers.has('navigation-area-selection-requested')).toBe(true),
		);

		mocked.handlers.get('navigation-area-selection-requested')?.({
			payload: { requestId: 7, planet: 'Arkadia', mapViewId: null },
		});
		await vi.waitFor(() => expect(mocked.show).toHaveBeenCalledOnce());
		expect(mocked.hide).not.toHaveBeenCalled();
		expect(mocked.emit).toHaveBeenCalledWith('navigation-area-selection-cancelled', {
			requestId: 7,
		});
		expect(controller.active).toBe(false);
		stop();
	});
});
