import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { MapPin, MapView, PlanetMap } from '$lib/api';
import { createMapsModel } from './mapsModel.svelte';

vi.mock('$lib/api', () => ({
	getPlanetMaps: vi.fn(),
	getMapPins: vi.fn(),
	getMapViews: vi.fn(),
	createMapPin: vi.fn(),
	createMapView: vi.fn(),
	updateMapPin: vi.fn(),
	renameMapView: vi.fn(),
	deleteMapPin: vi.fn(),
	deleteMapView: vi.fn(),
	planetMapImage: vi.fn(),
}));

import * as api from '$lib/api';

const mocked = vi.mocked(api);

function planet(overrides: Partial<PlanetMap> = {}): PlanetMap {
	return {
		name: 'Calypso',
		technicalName: 'Calypso',
		imageMime: 'image/jpeg',
		imageWidthPx: 4608,
		imageHeightPx: 4608,
		calibration: {
			tileOriginX: 2,
			tileOriginY: 3,
			tileWidth: 9,
			tileHeight: 9,
			unitsPerPixelX: 16,
			unitsPerPixelY: 16,
			bounds: { lonMin: 16384, lonMax: 90112, latMin: 24576, latMax: 98304 },
		},
		...overrides,
	};
}

function pin(overrides: Partial<MapPin> = {}): MapPin {
	return {
		id: 1,
		planet: 'Calypso',
		lon: 61400,
		lat: 75800,
		altitude: 103,
		name: 'Port Atlantis TP',
		icon: 'teleporter',
		kind: 'travel',
		radiusM: null,
		notes: null,
		sessionId: null,
		mapViewId: null,
		createdAt: 1_752_000_000,
		...overrides,
	};
}

function view(overrides: Partial<MapView> = {}): MapView {
	return {
		id: 7,
		planet: 'Calypso',
		name: 'New map',
		createdAt: 1_752_000_000,
		...overrides,
	};
}

beforeEach(() => {
	vi.clearAllMocks();
	mocked.planetMapImage.mockResolvedValue('data:image/jpeg;base64,xx');
	mocked.getMapPins.mockResolvedValue([]);
	mocked.getMapViews.mockResolvedValue([]);
});

describe('createMapsModel', () => {
	it('loads the catalogue and auto-selects the first planet', async () => {
		mocked.getPlanetMaps.mockResolvedValue([planet(), planet({ name: 'Arkadia' })]);
		mocked.getMapPins.mockResolvedValue([pin()]);
		const model = createMapsModel();
		await model.loadPlanets();

		expect(model.selected?.name).toBe('Calypso');
		expect(model.imageUrl).toBe('data:image/jpeg;base64,xx');
		expect(model.pins).toHaveLength(1);
		expect(model.loading).toBe(false);
		expect(mocked.getMapPins).toHaveBeenCalledWith('Calypso', null);
	});

	it('surfaces a load failure as the error state', async () => {
		mocked.getPlanetMaps.mockRejectedValue(new Error('nope'));
		const model = createMapsModel();
		await model.loadPlanets();
		expect(model.error).not.toBeNull();
		expect(model.loading).toBe(false);
	});

	it('an empty catalogue settles without a selection', async () => {
		mocked.getPlanetMaps.mockResolvedValue([]);
		const model = createMapsModel();
		await model.loadPlanets();
		expect(model.selected).toBeNull();
		expect(model.loading).toBe(false);
		expect(model.error).toBeNull();
	});

	it('pin CRUD keeps the local list in step', async () => {
		mocked.getPlanetMaps.mockResolvedValue([planet()]);
		const model = createMapsModel();
		await model.loadPlanets();

		mocked.createMapPin.mockResolvedValue(pin({ id: 7 }));
		await model.addPin({
			planet: 'Calypso',
			lon: 61400,
			lat: 75800,
			altitude: null,
			name: 'Port Atlantis TP',
			icon: 'teleporter',
			kind: 'travel',
			radiusM: null,
			notes: null,
			sessionId: null,
			mapViewId: null,
		});
		expect(model.pins.map((entry) => entry.id)).toEqual([7]);

		mocked.updateMapPin.mockResolvedValue(pin({ id: 7, name: 'PA' }));
		await model.editPin(7, { name: 'PA' } as never);
		expect(model.pins[0].name).toBe('PA');

		mocked.deleteMapPin.mockResolvedValue(undefined);
		await model.removePin(7);
		expect(model.pins).toHaveLength(0);
	});

	it('refreshes the selected planet pins after an overlay invalidation', async () => {
		mocked.getPlanetMaps.mockResolvedValue([planet()]);
		const model = createMapsModel();
		await model.loadPlanets();
		mocked.getMapPins.mockResolvedValueOnce([pin({ id: 9, name: 'Overlay pin' })]);
		await model.refreshPins();
		expect(mocked.getMapPins).toHaveBeenLastCalledWith('Calypso', null);
		expect(model.pins).toEqual([pin({ id: 9, name: 'Overlay pin' })]);
	});

	it('creates, selects, renames, and deletes an independent named map', async () => {
		mocked.getPlanetMaps.mockResolvedValue([planet()]);
		mocked.createMapView.mockResolvedValue(view());
		mocked.renameMapView.mockResolvedValue(view({ name: 'Trees' }));
		mocked.deleteMapView.mockResolvedValue(undefined);
		const model = createMapsModel();
		await model.loadPlanets();

		const created = await model.addView();
		expect(created).toEqual(view());
		expect(mocked.createMapView).toHaveBeenCalledWith('Calypso', 'New map');
		expect(mocked.getMapPins).toHaveBeenLastCalledWith('Calypso', 7);
		expect(model.selectedViewId).toBe(7);

		await model.renameView(7, 'Trees');
		expect(model.views).toEqual([view({ name: 'Trees' })]);

		await model.removeView(7);
		expect(model.views).toEqual([]);
		expect(model.selectedViewId).toBeNull();
		expect(mocked.getMapPins).toHaveBeenLastCalledWith('Calypso', null);
	});

	it('a stale selection cannot clobber a newer one', async () => {
		mocked.getPlanetMaps.mockResolvedValue([planet(), planet({ name: 'Arkadia' })]);
		const model = createMapsModel();
		await model.loadPlanets();

		let releaseSlow: (value: string) => void = () => {};
		mocked.planetMapImage.mockImplementationOnce(
			() => new Promise((resolve) => (releaseSlow = resolve)),
		);
		const slow = model.selectPlanet('Arkadia');
		mocked.planetMapImage.mockResolvedValueOnce('data:image/jpeg;base64,calypso');
		await model.selectPlanet('Calypso');
		releaseSlow('data:image/jpeg;base64,stale-arkadia');
		await slow;

		expect(model.selected?.name).toBe('Calypso');
		expect(model.imageUrl).toBe('data:image/jpeg;base64,calypso');
	});

	it('preserves the last-good map when a replacement fails to load', async () => {
		mocked.getPlanetMaps.mockResolvedValue([planet(), planet({ name: 'Arkadia' })]);
		mocked.getMapPins.mockResolvedValueOnce([pin()]);
		const model = createMapsModel();
		await model.loadPlanets();

		mocked.planetMapImage.mockRejectedValueOnce(new Error('raster unavailable'));
		await model.selectPlanet('Arkadia');

		expect(model.selected?.name).toBe('Calypso');
		expect(model.imageUrl).toBe('data:image/jpeg;base64,xx');
		expect(model.pins).toEqual([pin()]);
		expect(model.error).toBe('raster unavailable');
	});

	it('discards a created view result after the selection context changes', async () => {
		mocked.getPlanetMaps.mockResolvedValue([planet(), planet({ name: 'Arkadia' })]);
		const model = createMapsModel();
		await model.loadPlanets();

		let releaseCreate: (created: MapView) => void = () => {};
		mocked.createMapView.mockImplementationOnce(
			() => new Promise((resolve) => (releaseCreate = resolve)),
		);
		const pendingCreate = model.addView();
		await model.selectPlanet('Arkadia');
		releaseCreate(view());

		expect(await pendingCreate).toBeNull();
		expect(model.selected?.name).toBe('Arkadia');
		expect(model.views).toEqual([]);
	});

	it('does not let an old-view refresh overwrite a newly selected view', async () => {
		mocked.getPlanetMaps.mockResolvedValue([planet()]);
		mocked.getMapViews.mockResolvedValue([view()]);
		const model = createMapsModel();
		await model.loadPlanets();
		mocked.getMapPins.mockResolvedValueOnce([pin({ id: 7, mapViewId: 7 })]);
		await model.selectView(7);

		let releaseSelection: (pins: MapPin[]) => void = () => {};
		let releaseRefresh: (pins: MapPin[]) => void = () => {};
		mocked.getMapPins
			.mockImplementationOnce(() => new Promise((resolve) => (releaseSelection = resolve)))
			.mockImplementationOnce(() => new Promise((resolve) => (releaseRefresh = resolve)));
		const selection = model.selectView(null);
		const refresh = model.refreshPins();
		releaseSelection([pin({ id: 8, name: 'Default pin' })]);
		await selection;
		releaseRefresh([pin({ id: 9, mapViewId: 7, name: 'Old-view pin' })]);
		await refresh;

		expect(model.selectedViewId).toBeNull();
		expect(model.pins).toEqual([pin({ id: 8, name: 'Default pin' })]);
	});

	it('does not surface an old-view refresh failure against a newly selected view', async () => {
		mocked.getPlanetMaps.mockResolvedValue([planet()]);
		mocked.getMapViews.mockResolvedValue([view()]);
		const model = createMapsModel();
		await model.loadPlanets();
		await model.selectView(7);

		let releaseSelection: (pins: MapPin[]) => void = () => {};
		let rejectRefresh: (error: Error) => void = () => {};
		mocked.getMapPins
			.mockImplementationOnce(() => new Promise((resolve) => (releaseSelection = resolve)))
			.mockImplementationOnce(() => new Promise((_, reject) => (rejectRefresh = reject)));
		const selection = model.selectView(null);
		const refresh = model.refreshPins();
		releaseSelection([pin({ id: 8, name: 'Default pin' })]);
		await selection;
		rejectRefresh(new Error('stale refresh failed'));
		await refresh;

		expect(model.selectedViewId).toBeNull();
		expect(model.error).toBeNull();
	});
});
