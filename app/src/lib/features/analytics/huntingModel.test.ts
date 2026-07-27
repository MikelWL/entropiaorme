import { beforeEach, describe, expect, it, vi } from 'vitest';
import { activityArchive } from '$lib/activityArchive.svelte';
import type { AnalyticsHunting } from '$lib/api/commands.gen';
import { createHuntingModel } from './huntingModel.svelte';

vi.mock('$lib/api', () => ({
	getAnalyticsHunting: vi.fn(),
}));

vi.mock('$lib/preferences', () => ({
	getPreference: vi.fn(),
	setPreference: vi.fn(),
}));

import * as api from '$lib/api';
import * as prefs from '$lib/preferences';

const mocked = vi.mocked(api);
const mockedPrefs = vi.mocked(prefs);

function hunting(): AnalyticsHunting {
	return {
		mobComparisons: [
			{
				mobName: 'Atrox Young',
				sessions: 3,
				kills: 90,
				hours: 2,
				cycled: 200,
				pesPer100Ped: 1.5,
				lootRate: 0.9,
			},
			{
				mobName: 'Snablesnot',
				sessions: 1,
				kills: 40,
				hours: 1,
				cycled: 500,
				pesPer100Ped: 2.5,
				lootRate: 0.8,
			},
		],
		nameComparisons: [
			{
				sessionName: 'ARIS Dailies',
				sessions: 2,
				kills: 60,
				hours: 1.5,
				cycled: 300,
				pesPer100Ped: 2,
				lootRate: 0.85,
			},
		],
	};
}

beforeEach(() => {
	vi.clearAllMocks();
	mockedPrefs.setPreference.mockResolvedValue(undefined);
	activityArchive.current = { mobs: [], names: [] };
});

describe('loadData', () => {
	it('loads the comparison tables', async () => {
		mocked.getAnalyticsHunting.mockResolvedValue(hunting());
		const model = createHuntingModel();
		await model.loadData();

		expect(model.data?.mobComparisons).toHaveLength(2);
		expect(model.loading).toBe(false);
		expect(model.error).toBeNull();
	});

	it('surfaces a load failure', async () => {
		mocked.getAnalyticsHunting.mockRejectedValue(new Error('backend unreachable'));
		const model = createHuntingModel();
		await model.loadData();
		expect(model.error).toBe('backend unreachable');
		expect(model.data).toBeNull();
	});
});

describe('sorted projections', () => {
	it('defaults to cycled descending and re-sorts on key or direction change', async () => {
		mocked.getAnalyticsHunting.mockResolvedValue(hunting());
		const model = createHuntingModel();
		await model.loadData();

		expect(model.mobSortKey).toBe('cycled');
		expect(model.mobSortDir).toBe('desc');
		expect(model.sortedMobs.map((m) => m.mobName)).toEqual(['Snablesnot', 'Atrox Young']);

		model.mobSortDir = 'asc';
		expect(model.sortedMobs.map((m) => m.mobName)).toEqual(['Atrox Young', 'Snablesnot']);

		model.mobSortKey = 'mobName';
		expect(model.sortedMobs.map((m) => m.mobName)).toEqual(['Atrox Young', 'Snablesnot']);
	});

	it('keeps the filtered order untouched when no sort key is set', async () => {
		mocked.getAnalyticsHunting.mockResolvedValue(hunting());
		const model = createHuntingModel();
		await model.loadData();

		model.mobSortKey = undefined;
		expect(model.sortedMobs.map((m) => m.mobName)).toEqual(['Atrox Young', 'Snablesnot']);
	});
});

describe('archive split', () => {
	it('splits rows between the main and archive views per kind', async () => {
		mocked.getAnalyticsHunting.mockResolvedValue(hunting());
		const model = createHuntingModel();
		await model.loadData();

		await model.onArchiveConfirm('mob', 'Atrox Young');
		expect(model.confirmKey).toBeNull();
		expect(model.sortedMobs.map((m) => m.mobName)).toEqual(['Snablesnot']);
		// The other kind is untouched.
		expect(model.sortedNames).toHaveLength(1);

		model.viewMode = 'archive';
		expect(model.sortedMobs.map((m) => m.mobName)).toEqual(['Atrox Young']);
		expect(model.sortedNames).toHaveLength(0);

		await model.onUnarchiveConfirm('mob', 'Atrox Young');
		expect(model.sortedMobs).toHaveLength(0);
		model.viewMode = 'main';
		expect(model.sortedMobs).toHaveLength(2);
	});

	it('surfaces an archive persistence failure and clears a stale error on entry', async () => {
		mocked.getAnalyticsHunting.mockResolvedValue(hunting());
		mockedPrefs.setPreference.mockRejectedValueOnce(new Error('disk full'));
		const model = createHuntingModel();
		await model.loadData();

		model.confirmKey = 'mob:Atrox Young';
		await model.onArchiveConfirm('mob', 'Atrox Young');
		expect(model.error).toBe('disk full');
		expect(model.confirmKey).toBeNull();

		await model.onUnarchiveConfirm('mob', 'Atrox Young');
		expect(model.error).toBeNull();
	});
});
