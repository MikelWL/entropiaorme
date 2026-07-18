import { describe, expect, it } from 'vitest';
import type { MapPin } from '$lib/api';
import { filterMapPins, parseGamePointInput } from './mapTools';

const pin = (overrides: Partial<MapPin>): MapPin => ({
	id: 1,
	planet: 'Calypso',
	lon: 61_400,
	lat: 75_800,
	altitude: null,
	name: 'Port Atlantis',
	icon: 'teleporter',
	kind: 'travel',
	radiusM: null,
	notes: 'South coast',
	sessionId: null,
	createdAt: 1,
	...overrides,
});

describe('map tools', () => {
	it('parses comma-separated game coordinates', () => {
		expect(parseGamePointInput(' 61400, 75800 ')).toEqual({ lon: 61_400, lat: 75_800 });
		expect(parseGamePointInput('61400 75800')).toBeNull();
		expect(parseGamePointInput('north, east')).toBeNull();
	});

	it('filters pins by name or notes without changing an empty result set', () => {
		const pins = [pin({}), pin({ id: 2, name: 'Longu nest', notes: null })];
		expect(filterMapPins(pins, 'atlantis')).toEqual([pins[0]]);
		expect(filterMapPins(pins, 'SOUTH')).toEqual([pins[0]]);
		expect(filterMapPins(pins, 'missing')).toEqual([]);
		expect(filterMapPins(pins, '  ')).toBe(pins);
	});
});
