import { describe, expect, it } from 'vitest';

import type { PlanetMapCalibration } from '$lib/api';
import { formatGamePoint, gameToImage, imageToGame, inBounds } from './coords';

/** Calypso's bundled record: origin tile (2,3), 9x9 tiles, 4608 px. */
const CALYPSO: PlanetMapCalibration = {
	tileOriginX: 2,
	tileOriginY: 3,
	tileWidth: 9,
	tileHeight: 9,
	unitsPerPixelX: 16,
	unitsPerPixelY: 16,
	bounds: { lonMin: 16384, lonMax: 90112, latMin: 24576, latMax: 98304 },
};

/** ARIS: the anisotropic map (512x1536 raster over a 3x4-tile window). */
const ARIS: PlanetMapCalibration = {
	tileOriginX: 3,
	tileOriginY: 0,
	tileWidth: 3,
	tileHeight: 4,
	unitsPerPixelX: 48,
	unitsPerPixelY: 32768 / 1536,
	bounds: { lonMin: 24576, lonMax: 49152, latMin: 0, latMax: 32768 },
};

describe('gameToImage', () => {
	it('places the Port Atlantis sanity anchor', () => {
		const px = gameToImage(CALYPSO, { lon: 61400, lat: 75800 });
		expect(px.x).toBeCloseTo(2813.5, 1);
		expect(px.y).toBeCloseTo(1406.5, 1);
	});

	it('maps the window corners to the image corners (y flipped)', () => {
		// South-west game corner = bottom-left image corner.
		expect(gameToImage(CALYPSO, { lon: 16384, lat: 24576 })).toEqual({ x: 0, y: 4608 });
		// North-east game corner = top-right image corner.
		expect(gameToImage(CALYPSO, { lon: 90112, lat: 98304 })).toEqual({ x: 4608, y: 0 });
	});

	it('scales the axes independently on an anisotropic map', () => {
		// The window centre lands at the raster centre on both axes.
		const centre = gameToImage(ARIS, { lon: (24576 + 49152) / 2, lat: 16384 });
		expect(centre.x).toBeCloseTo(256, 5);
		expect(centre.y).toBeCloseTo(768, 5);
	});
});

describe('imageToGame', () => {
	it('inverts gameToImage exactly', () => {
		for (const cal of [CALYPSO, ARIS]) {
			const original = { lon: 33333, lat: 28001 };
			const roundTripped = imageToGame(cal, gameToImage(cal, original));
			expect(roundTripped.lon).toBeCloseTo(original.lon, 6);
			expect(roundTripped.lat).toBeCloseTo(original.lat, 6);
		}
	});
});

describe('inBounds', () => {
	it('accepts the window edges and refuses beyond them', () => {
		expect(inBounds(CALYPSO, { lon: 16384, lat: 24576 })).toBe(true);
		expect(inBounds(CALYPSO, { lon: 90112, lat: 98304 })).toBe(true);
		expect(inBounds(CALYPSO, { lon: 16383, lat: 24576 })).toBe(false);
		expect(inBounds(CALYPSO, { lon: 61400, lat: 99999 })).toBe(false);
	});
});

describe('formatGamePoint', () => {
	it('renders the readout shape the game shows', () => {
		expect(formatGamePoint({ lon: 61400.4, lat: 75799.6 })).toBe('61400, 75800');
	});
});
