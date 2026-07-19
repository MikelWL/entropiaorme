/**
 * Game-unit <-> image-pixel transforms over a planet map's tile-grid
 * calibration. Pure functions; the calibration record comes from the
 * bundled catalogue (`planet_maps_list`).
 *
 * Entropia positions are `(longitude, latitude)` with longitude growing
 * eastward (image x direction) and latitude growing northward, opposite
 * of image y, which grows downward: hence the y flip. Each map is an
 * axis-aligned window onto the game's global tile grid (8192 game units
 * per tile); the per-axis units-per-pixel scales come pre-derived, so a
 * map whose raster aspect ratio does not match its tile window (ARIS)
 * still places correctly on both axes.
 */

import type { PlanetMapCalibration } from '$lib/api';

/** A position in game units. */
export interface GamePoint {
	lon: number;
	lat: number;
}

/** A position in image pixels (origin top-left, y grows down). */
export interface ImagePoint {
	x: number;
	y: number;
}

/** Game units at the image's left edge. */
function offsetLon(cal: PlanetMapCalibration): number {
	return cal.bounds.lonMin;
}

/** Game units at the image's BOTTOM edge. */
function offsetLat(cal: PlanetMapCalibration): number {
	return cal.bounds.latMin;
}

/** North-south extent in game units. */
function spanLat(cal: PlanetMapCalibration): number {
	return cal.bounds.latMax - cal.bounds.latMin;
}

/** Game units -> image pixels (note the y flip). */
export function gameToImage(cal: PlanetMapCalibration, point: GamePoint): ImagePoint {
	return {
		x: (point.lon - offsetLon(cal)) / cal.unitsPerPixelX,
		y: (spanLat(cal) - (point.lat - offsetLat(cal))) / cal.unitsPerPixelY,
	};
}

/** Image pixels -> game units (the inverse of {@link gameToImage}). */
export function imageToGame(cal: PlanetMapCalibration, point: ImagePoint): GamePoint {
	return {
		lon: point.x * cal.unitsPerPixelX + offsetLon(cal),
		lat: spanLat(cal) - point.y * cal.unitsPerPixelY + offsetLat(cal),
	};
}

/** Whether a coordinate lies inside the map's calibrated window. */
export function inBounds(cal: PlanetMapCalibration, point: GamePoint): boolean {
	return (
		point.lon >= cal.bounds.lonMin &&
		point.lon <= cal.bounds.lonMax &&
		point.lat >= cal.bounds.latMin &&
		point.lat <= cal.bounds.latMax
	);
}

/** A coordinate pair as the game displays it: `61400, 75800`. */
export function formatGamePoint(point: GamePoint): string {
	return `${Math.round(point.lon)}, ${Math.round(point.lat)}`;
}
