// @vitest-environment happy-dom

import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { MapPin } from '$lib/api';
import { createMapsController } from './mapsController.svelte';
import type { MapsModel } from './mapsModel.svelte';

vi.mock('$lib/api', () => ({ getNearbyMapPin: vi.fn() }));

const pin: MapPin = {
	id: 7,
	planet: 'Calypso',
	lon: 1,
	lat: 2,
	altitude: null,
	name: 'Tree cluster',
	icon: 'tree',
	kind: 'tree',
	radiusM: null,
	notes: null,
	sessionId: null,
	mapViewId: null,
	createdAt: 1,
	lastVisitedAt: null,
	cooldownUntil: null,
	pinConfigId: 1,
	colour: '#22c55e',
	cooldownColour: '#f59e0b',
	category: 'special',
	specialKind: 'tree',
};

beforeEach(() => vi.restoreAllMocks());

describe('maps pin lifecycle controller', () => {
	it('confirms before deleting an individual pin', async () => {
		const removePin = vi.fn().mockResolvedValue(undefined);
		const controller = createMapsController({ removePin } as unknown as MapsModel);
		const confirmDelete = vi.spyOn(window, 'confirm').mockReturnValue(false);

		await controller.deletePin(pin);
		expect(confirmDelete).toHaveBeenCalledWith('Delete pin "Tree cluster"? This cannot be undone.');
		expect(removePin).not.toHaveBeenCalled();

		confirmDelete.mockReturnValue(true);
		await controller.deletePin(pin);
		expect(removePin).toHaveBeenCalledWith(7);
		expect(controller.feedback).toBe('Pin "Tree cluster" deleted.');
	});
});
