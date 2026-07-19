import type { MapPin } from '$lib/api';
import type { GamePoint } from './coords';

export interface MapFocusRequest {
	point: GamePoint;
	nonce: number;
}

export function filterMapPins(pins: MapPin[], query: string): MapPin[] {
	const needle = query.trim().toLowerCase();
	if (!needle) return pins;
	return pins.filter((pin) => `${pin.name}\n${pin.notes ?? ''}`.toLowerCase().includes(needle));
}
