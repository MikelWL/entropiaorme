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
	mapViewId: null,
	createdAt: 1_752_000_000,
	lastVisitedAt: null,
	cooldownUntil: null,
	pinConfigId: null,
	colour: null,
	cooldownColour: null,
	category: null,
	specialKind: null,
};

function setup() {
	const onmapclick = vi.fn();
	render(MapViewer, {
		props: {
			planet,
			planets: [planet],
			imageUrl: 'data:image/jpeg;base64,xx',
			pins: [pin],
			views: [],
			selectedViewId: null,
			onmapclick,
			oncopywaypoint: vi.fn(),
			oneditpin: vi.fn(),
			ondeletepin: vi.fn(),
			oncooldownpin: vi.fn(),
			onselectplanet: vi.fn(),
			onselectview: vi.fn(),
			onaddview: vi.fn().mockResolvedValue(null),
			onrenameview: vi.fn().mockResolvedValue(true),
			ondeleteview: vi.fn().mockResolvedValue(true),
		},
	});
	return { onmapclick };
}

describe('MapViewer scalable interaction surface', () => {
	it('keeps the map keyboard operable without creating one DOM control per pin', () => {
		setup();
		const map = screen.getByRole('application', { name: /Calypso map/ });
		expect(map.getAttribute('tabindex')).toBe('0');
		expect(screen.queryByRole('button', { name: /Port Atlantis TP/ })).toBeNull();
	});

	it('offers explicit icon and precision-dot modes with automatic density rendering', async () => {
		setup();
		const display = screen.getByRole('combobox', { name: 'Pin display' }) as HTMLSelectElement;
		expect(display.value).toBe('auto');
		await fireEvent.change(display, { target: { value: 'precision' } });
		expect(display.value).toBe('precision');
	});

	it('retains fit and incremental zoom controls as an escape from extreme inspection zoom', () => {
		setup();
		expect(screen.getByRole('button', { name: 'Zoom in' })).toBeTruthy();
		expect(screen.getByRole('button', { name: 'Zoom out' })).toBeTruthy();
		expect(screen.getByRole('button', { name: 'Fit map to view' })).toBeTruthy();
	});

	it('discloses the map-asset attribution note only when its badge is toggled', async () => {
		setup();
		const badge = screen.getByRole('button', { name: /Map asset by Entropia Nexus/ });
		expect(badge.getAttribute('aria-expanded')).toBe('false');
		expect(screen.queryByRole('dialog', { name: 'Map asset attribution' })).toBeNull();

		await fireEvent.click(badge);
		expect(badge.getAttribute('aria-expanded')).toBe('true');
		const link = screen.getByRole('link', { name: 'Entropia Nexus' });
		expect(link.getAttribute('href')).toBe('https://entropianexus.com/maps');

		await fireEvent.click(badge);
		expect(screen.queryByRole('dialog', { name: 'Map asset attribution' })).toBeNull();
	});
});
