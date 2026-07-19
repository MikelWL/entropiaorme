import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { PlanetMap } from '$lib/api';
import type { MapsModel } from './mapsModel.svelte';

const seams = vi.hoisted(() => ({
	listen: vi.fn().mockResolvedValue(() => {}),
}));

vi.mock('@tauri-apps/api/event', () => ({ listen: seams.listen }));
vi.mock('./cartographyOverlay.svelte', () => ({
	MAP_PINS_CHANGED_EVENT: 'map-pins-changed',
}));

import { startMapsCartographySync } from './mapsCartographySync';

const planet: PlanetMap = {
	name: 'Calypso',
	technicalName: 'Calypso',
	imageMime: 'image/jpeg',
	imageWidthPx: 4608,
	imageHeightPx: 4608,
	calibration: null,
};

function model() {
	return {
		selected: planet,
		refreshPins: vi.fn().mockResolvedValue(undefined),
	};
}

beforeEach(() => {
	vi.clearAllMocks();
});

describe('startMapsCartographySync', () => {
	it('refreshes pins when the overlay drops one on the current planet', async () => {
		const m = model();
		const stop = startMapsCartographySync(m as unknown as MapsModel);

		await vi.waitFor(() =>
			expect(seams.listen).toHaveBeenCalledWith('map-pins-changed', expect.any(Function)),
		);
		const pinsListener = seams.listen.mock.calls.find(
			([eventName]) => eventName === 'map-pins-changed',
		)?.[1] as (event: { payload: unknown }) => void;

		pinsListener({ payload: { planet: 'Calypso' } });
		expect(m.refreshPins).toHaveBeenCalledTimes(1);

		pinsListener({ payload: { planet: 'Arkadia' } });
		expect(m.refreshPins).toHaveBeenCalledTimes(1);
		stop();
	});
});
