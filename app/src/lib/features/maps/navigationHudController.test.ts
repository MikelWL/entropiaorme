import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocked = vi.hoisted(() => ({
	beginSelection: vi.fn(),
	startNavigation: vi.fn(),
	scanCoordinates: vi.fn(),
	emit: vi.fn(),
}));

vi.mock('@tauri-apps/api/event', () => ({ emit: mocked.emit }));
vi.mock('$lib/preferences', () => ({
	getPreference: vi.fn((_key: string, fallback: unknown) => Promise.resolve(fallback)),
	setPreference: vi.fn(),
}));
vi.mock('$lib/api', () => ({
	beginNavigationAreaSelection: mocked.beginSelection,
	endNavigation: vi.fn(),
	getNavigationSnapshot: vi.fn().mockResolvedValue(null),
	hideNavigationOverlays: vi.fn(),
	markNavigationVisited: vi.fn(),
	resolveNavigationHarvest: vi.fn(),
	scanMapCoordinates: mocked.scanCoordinates,
	skipNavigationStop: vi.fn(),
	startNavigation: mocked.startNavigation,
	undoNavigationStop: vi.fn(),
	updateNavigationPosition: vi.fn(),
}));
vi.mock('./cartographyOverlay.svelte', () => ({
	acceptCartographyContextBroadcast: (value: unknown) => value,
	cartographyScanFailureMessage: vi.fn(() => 'scan failed'),
}));

import { createNavigationHudController } from './navigationHudController.svelte';

beforeEach(() => {
	vi.clearAllMocks();
	mocked.beginSelection.mockResolvedValue(undefined);
	mocked.scanCoordinates.mockResolvedValue({
		status: 'read',
		lon: 100,
		lat: 200,
		altitude: null,
	});
	mocked.startNavigation.mockResolvedValue({ status: 'active', stops: [] });
});

describe('navigation HUD route-area scope', () => {
	it('keeps all trees as the default and accepts only the current selection request', async () => {
		const controller = createNavigationHudController();
		controller.applyContext({ planet: 'Calypso', mapViewId: null });
		await controller.chooseRouteArea();
		expect(mocked.beginSelection).toHaveBeenCalledWith(1, 'Calypso', null);

		controller.applyRouteAreaSelection({
			requestId: 99,
			planet: 'Calypso',
			mapViewId: null,
			pinIds: [9],
		});
		expect(controller.selectedTreeCount).toBeNull();

		controller.applyRouteAreaSelection({
			requestId: 1,
			planet: 'Calypso',
			mapViewId: null,
			pinIds: [9, 3, 9],
		});
		expect(controller.selectedTreeCount).toBe(2);
	});

	it('starts with the exact selection and consumes it back to the all-trees default', async () => {
		const controller = createNavigationHudController();
		controller.applyContext({ planet: 'Calypso', mapViewId: 7 });
		await controller.chooseRouteArea();
		controller.applyRouteAreaSelection({
			requestId: 1,
			planet: 'Calypso',
			mapViewId: 7,
			pinIds: [8, 2],
		});
		await controller.captureStart();
		await controller.beginRoute();

		expect(mocked.startNavigation).toHaveBeenCalledWith('Calypso', 7, 100, 200, [2, 8], 'f8');
		expect(controller.selectedTreeCount).toBeNull();
	});

	it('returns an area-scoped draft to all trees explicitly', async () => {
		const controller = createNavigationHudController();
		controller.applyContext({ planet: 'Calypso', mapViewId: null });
		await controller.chooseRouteArea();
		controller.applyRouteAreaSelection({
			requestId: 1,
			planet: 'Calypso',
			mapViewId: null,
			pinIds: [4],
		});
		controller.useAllTrees();
		expect(controller.selectedTreeCount).toBeNull();
		expect(mocked.emit).toHaveBeenCalledWith('navigation-area-selection-reset');
	});
});
