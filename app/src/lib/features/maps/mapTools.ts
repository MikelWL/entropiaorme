import type { MapPin } from '$lib/api';
import type { GamePoint } from './coords';

export interface MapFocusRequest {
	point: GamePoint;
	nonce: number;
}

/** Parse the coordinate form used by the game and waypoint readouts. */
export function parseGamePointInput(value: string): GamePoint | null {
	const match = /^\s*(\d+(?:\.\d+)?)\s*,\s*(\d+(?:\.\d+)?)\s*$/.exec(value);
	if (!match) return null;
	const lon = Number(match[1]);
	const lat = Number(match[2]);
	return Number.isFinite(lon) && Number.isFinite(lat) ? { lon, lat } : null;
}

/** Current-planet pin filtering over the user-authored searchable fields. */
export function filterMapPins(pins: MapPin[], query: string): MapPin[] {
	const needle = query.trim().toLowerCase();
	if (!needle) return pins;
	return pins.filter((pin) => `${pin.name}\n${pin.notes ?? ''}`.toLowerCase().includes(needle));
}
