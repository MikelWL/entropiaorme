// @vitest-environment happy-dom

import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import type { MapPin, PlanetMap } from '$lib/api';
import MapViewer from './MapViewer.svelte';

const planet: PlanetMap = {
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
};

const pin: MapPin = {
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
	createdAt: 1_752_000_000,
};

function setup() {
	const onmapclick = vi.fn();
	const oncopywaypoint = vi.fn().mockResolvedValue('Waypoint copied.');
	const oneditpin = vi.fn();
	const ondeletepin = vi.fn();
	render(MapViewer, {
		props: {
			planet,
			imageUrl: 'data:image/jpeg;base64,xx',
			pins: [pin],
			onmapclick,
			oncopywaypoint,
			oneditpin,
			ondeletepin,
		},
	});
	return { onmapclick, oncopywaypoint, oneditpin };
}

describe('MapViewer pin interactions', () => {
	it('copies from marker activation while hover retains the detail card', async () => {
		const { oncopywaypoint } = setup();
		const marker = screen.getByRole('button', { name: 'Copy waypoint for Port Atlantis TP' });

		await fireEvent.mouseEnter(marker);
		expect(screen.getByText('Click the pin to copy its waypoint.')).toBeTruthy();
		expect(screen.queryByRole('button', { name: 'Copy waypoint' })).toBeNull();

		await fireEvent.click(marker);
		expect(oncopywaypoint).toHaveBeenCalledWith(pin);
		expect(await screen.findByText('Waypoint copied.')).toBeTruthy();
		expect(screen.queryByText('Click the pin to copy its waypoint.')).toBeNull();
	});

	it('contains card gestures so edit cannot become a map click', async () => {
		const { onmapclick, oneditpin } = setup();
		await fireEvent.mouseEnter(
			screen.getByRole('button', { name: 'Copy waypoint for Port Atlantis TP' }),
		);
		const edit = screen.getByRole('button', { name: 'Edit' });

		await fireEvent.pointerDown(edit, { button: 0, pointerId: 1 });
		await fireEvent.pointerUp(edit, { button: 0, pointerId: 1 });
		await fireEvent.click(edit);

		expect(oneditpin).toHaveBeenCalledWith(pin);
		expect(onmapclick).not.toHaveBeenCalled();
	});
});
