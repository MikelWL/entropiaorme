import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { MapView, PlanetMap } from '$lib/api';
import type { MapsModel } from './mapsModel.svelte';

const seams = vi.hoisted(() => ({
	config: {
		current: {
			planet: 'Calypso' as string | null,
			mapViewId: 7 as number | null,
			buttons: [],
		},
	},
	setConfig: vi.fn().mockResolvedValue(undefined),
	listen: vi.fn().mockResolvedValue(() => {}),
}));

vi.mock('@tauri-apps/api/event', () => ({ listen: seams.listen }));
vi.mock('./cartographyOverlay.svelte', () => ({
	acceptCartographyOverlayBroadcast: vi.fn(),
	CARTOGRAPHY_OVERLAY_CHANGED_EVENT: 'cartography-overlay-changed',
	cartographyOverlayConfig: seams.config,
	MAP_PINS_CHANGED_EVENT: 'map-pins-changed',
	setCartographyOverlayConfig: seams.setConfig,
}));

import { startMapsCartographySync } from './mapsCartographySync';

const planet: PlanetMap = {
	name: 'Calypso',
	technicalName: 'Calypso',
	imageMime: 'image/jpeg',
	imageWidthPx: 4608,
	imageHeightPx: 4608,
	calibration: null,
};

const view: MapView = {
	id: 7,
	planet: 'Calypso',
	name: 'Trees',
	createdAt: 1_752_000_000,
};

function modelWithViews(views: MapView[]) {
	const model = {
		planets: [planet],
		selected: planet,
		views,
		selectedViewId: null as number | null,
		loadPlanets: vi.fn().mockResolvedValue(undefined),
		selectPlanet: vi.fn().mockResolvedValue(undefined),
		selectView: vi.fn(async (id: number | null) => {
			model.selectedViewId = id;
		}),
		refreshPins: vi.fn().mockResolvedValue(undefined),
	};
	return model;
}

beforeEach(() => {
	vi.clearAllMocks();
	seams.config.current = { planet: 'Calypso', mapViewId: 7, buttons: [] };
});

describe('startMapsCartographySync', () => {
	it('restores the persisted named map into the central Maps model', async () => {
		const model = modelWithViews([view]);
		const stop = startMapsCartographySync(model as unknown as MapsModel);

		await vi.waitFor(() => expect(model.selectView).toHaveBeenCalledWith(7));
		expect(seams.setConfig).not.toHaveBeenCalled();
		stop();
	});

	it('falls back to Default when the persisted named map no longer exists', async () => {
		const model = modelWithViews([]);
		const stop = startMapsCartographySync(model as unknown as MapsModel);

		await vi.waitFor(() =>
			expect(seams.setConfig).toHaveBeenCalledWith({
				planet: 'Calypso',
				mapViewId: null,
				buttons: [],
			}),
		);
		stop();
	});
});
