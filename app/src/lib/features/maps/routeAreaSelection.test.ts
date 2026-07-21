import { describe, expect, it, vi } from 'vitest';
import type { MapPin, PlanetMapCalibration } from '$lib/api';

vi.mock('@tauri-apps/api/event', () => ({ emit: vi.fn() }));

import {
	acceptRouteAreaSelectionRequest,
	acceptRouteAreaSelectionResult,
	clampImageRect,
	type ImageRect,
	imageRectContains,
	normaliseImageRect,
	selectedMapPinIds,
	selectedRoutePinIds,
	selectedTreePinIds,
} from './routeAreaSelection';

const calibration: PlanetMapCalibration = {
	tileOriginX: 0,
	tileOriginY: 0,
	tileWidth: 1,
	tileHeight: 1,
	unitsPerPixelX: 1,
	unitsPerPixelY: 1,
	bounds: { lonMin: 0, lonMax: 100, latMin: 0, latMax: 100 },
};

function pin(id: number, lon: number, lat: number, patch: Partial<MapPin> = {}): MapPin {
	return {
		id,
		planet: 'Calypso',
		lon,
		lat,
		altitude: null,
		name: `Tree ${id}`,
		icon: 'tree',
		kind: 'tree',
		radiusM: null,
		notes: null,
		sessionId: null,
		mapViewId: null,
		createdAt: 0,
		lastVisitedAt: null,
		cooldownUntil: null,
		pinConfigId: 1,
		colour: '#22c55e',
		cooldownColour: '#f59e0b',
		category: 'special',
		specialKind: 'tree',
		...patch,
	};
}

describe('route-area selection geometry', () => {
	it('normalises reverse drags and clips them to the raster', () => {
		const normalised = normaliseImageRect({ x: 90, y: 70 }, { x: -10, y: 20 });
		expect(clampImageRect(normalised, 80, 60)).toEqual({
			left: 0,
			top: 20,
			right: 80,
			bottom: 60,
		});
	});

	it('treats rectangle edges as selected', () => {
		const rect: ImageRect = { left: 10, top: 20, right: 30, bottom: 40 };
		expect(imageRectContains(rect, { x: 10, y: 40 })).toBe(true);
		expect(imageRectContains(rect, { x: 9.99, y: 40 })).toBe(false);
	});

	it('returns sorted unique eligible tree ids from the union of regions', () => {
		const regions = [
			{ left: 5, top: 65, right: 35, bottom: 95 },
			{ left: 25, top: 55, right: 45, bottom: 75 },
		];
		const pins = [
			pin(3, 30, 30),
			pin(1, 10, 10),
			pin(2, 40, 40, { cooldownUntil: 101 }),
			pin(4, 20, 20, { specialKind: null, category: 'generic' }),
		];
		expect(selectedRoutePinIds(pins, calibration, regions, 100)).toEqual([1, 3]);
	});

	it('selects every pin for map actions but only trees for cooldown', () => {
		const regions = [{ left: 0, top: 0, right: 100, bottom: 100 }];
		const pins = [
			pin(3, 30, 30),
			pin(1, 10, 10, { specialKind: null, category: 'generic' }),
			pin(2, 200, 200),
		];
		const selected = selectedMapPinIds(pins, calibration, regions);
		expect(selected).toEqual([1, 3]);
		expect(selectedTreePinIds(pins, selected)).toEqual([3]);
	});
});

describe('route-area selection event boundary', () => {
	it('accepts a typed request and normalises a result allow-list', () => {
		expect(
			acceptRouteAreaSelectionRequest({ requestId: 2, planet: ' Calypso ', mapViewId: null }),
		).toEqual({ requestId: 2, planet: 'Calypso', mapViewId: null });
		expect(
			acceptRouteAreaSelectionResult({
				requestId: 2,
				planet: 'Calypso',
				mapViewId: null,
				pinIds: [9, 3, 9],
			}),
		).toEqual({ requestId: 2, planet: 'Calypso', mapViewId: null, pinIds: [3, 9] });
	});

	it('rejects malformed and empty event payloads', () => {
		expect(acceptRouteAreaSelectionRequest({ requestId: 0, planet: 'Calypso' })).toBeNull();
		expect(
			acceptRouteAreaSelectionResult({
				requestId: 1,
				planet: 'Calypso',
				mapViewId: null,
				pinIds: [],
			}),
		).toBeNull();
	});
});
