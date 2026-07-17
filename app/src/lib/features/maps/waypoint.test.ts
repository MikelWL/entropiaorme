import { describe, expect, it } from 'vitest';

import { formatWaypoint, sanitiseWaypointLabel } from './waypoint';

describe('formatWaypoint', () => {
	it('builds the paste-ready string with rounded coordinates', () => {
		expect(
			formatWaypoint({
				technicalName: 'Calypso',
				lon: 61400.4,
				lat: 75799.6,
				altitude: 103.2,
				label: 'Port Atlantis TP',
			}),
		).toBe('/wp [Calypso,61400,75800,103,Port Atlantis TP]');
	});

	it('defaults a missing altitude to 0', () => {
		expect(
			formatWaypoint({
				technicalName: 'Planet Toulan',
				lon: 132000,
				lat: 91000,
				altitude: null,
				label: 'Spot',
			}),
		).toBe('/wp [Planet Toulan,132000,91000,0,Spot]');
	});

	it('refuses a planet without a technical name', () => {
		expect(
			formatWaypoint({ technicalName: null, lon: 1, lat: 2, altitude: 3, label: 'x' }),
		).toBeNull();
	});
});

describe('sanitiseWaypointLabel', () => {
	it('neutralises the waypoint delimiters', () => {
		expect(sanitiseWaypointLabel('Ore [big, rich]  vein')).toBe('Ore big; rich vein');
	});
});
