import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { ActivityRecommenderResult, RecommenderActivity } from '$lib/api/commands.gen';
import type { PageErrorSlot } from './optimizerModel.svelte';
import { createRecommenderModel } from './recommenderModel.svelte';

vi.mock('$lib/api', () => ({
	getActivityRecommender: vi.fn(),
}));

import * as api from '$lib/api';

const mocked = vi.mocked(api);

function activity(overrides: Partial<RecommenderActivity> = {}): RecommenderActivity {
	return {
		activity: 'Resource Gatherer',
		professions: ['Resource Gatherer'],
		pesToPlusOne: 29.2,
		gainAtCap: 6.39,
		series: [0, 0.9, 6.39],
		contributors: [{ name: 'Analysis', currentLevel: 12, levelGain: 615, targetGain: 2.46 }],
		...overrides,
	};
}

function recommenderResult(
	overrides: Partial<ActivityRecommenderResult> = {},
): ActivityRecommenderResult {
	return {
		pesCap: 1000,
		sampleStep: 20,
		direct: activity({ activity: 'Animal Looter', pesToPlusOne: 8.1, gainAtCap: 15.8 }),
		candidates: [
			activity(),
			activity({ activity: 'Gardener', pesToPlusOne: 31.4, gainAtCap: 5.06 }),
		],
		...overrides,
	};
}

function makeModel(): { model: ReturnType<typeof createRecommenderModel>; errors: PageErrorSlot } {
	const errors: PageErrorSlot = { error: null };
	return { model: createRecommenderModel(errors), errors };
}

beforeEach(() => {
	vi.clearAllMocks();
});

describe('load', () => {
	it('clears the result without an API call for a none target', async () => {
		const { model } = makeModel();
		mocked.getActivityRecommender.mockResolvedValue(recommenderResult());
		await model.load({ kind: 'profession', name: 'Animal Looter' });
		expect(model.result).not.toBeNull();

		await model.load({ kind: 'none' });
		expect(model.result).toBeNull();
		expect(model.selected).toBeNull();
		expect(mocked.getActivityRecommender).toHaveBeenCalledTimes(1);
	});

	it('queries a single profession and selects the top candidate', async () => {
		const { model, errors } = makeModel();
		mocked.getActivityRecommender.mockResolvedValue(recommenderResult());
		await model.load({ kind: 'profession', name: 'Animal Looter' });
		expect(mocked.getActivityRecommender).toHaveBeenCalledWith({
			target: 'profession',
			professions: ['Animal Looter'],
		});
		expect(model.selected?.activity).toBe('Resource Gatherer');
		expect(model.loading).toBe(false);
		expect(errors.error).toBeNull();
	});

	it('expands a family target into its member professions', async () => {
		const { model } = makeModel();
		mocked.getActivityRecommender.mockResolvedValue(recommenderResult({ direct: null }));
		await model.load({ kind: 'family', key: 'looter' });
		expect(mocked.getActivityRecommender).toHaveBeenCalledWith({
			target: 'profession',
			professions: ['Animal Looter', 'Mutant Looter', 'Robot Looter'],
		});
	});

	it('queries HP with no professions', async () => {
		const { model } = makeModel();
		mocked.getActivityRecommender.mockResolvedValue(recommenderResult({ direct: null }));
		await model.load({ kind: 'hp' });
		expect(mocked.getActivityRecommender).toHaveBeenCalledWith({
			target: 'hp',
			professions: [],
		});
	});

	it('routes a soft error to the error slot and keeps the result empty', async () => {
		const { model, errors } = makeModel();
		mocked.getActivityRecommender.mockResolvedValue(
			recommenderResult({ error: "Profession 'Nonsense' not found", candidates: [] }),
		);
		await model.load({ kind: 'profession', name: 'Nonsense' });
		expect(model.result).toBeNull();
		expect(errors.error).toBe("Profession 'Nonsense' not found");
	});

	it('surfaces a thrown failure and lands the result on null', async () => {
		const { model, errors } = makeModel();
		mocked.getActivityRecommender.mockResolvedValue(recommenderResult());
		await model.load({ kind: 'profession', name: 'Animal Looter' });
		expect(model.result).not.toBeNull();

		mocked.getActivityRecommender.mockRejectedValue(new Error('backend unreachable'));
		await model.load({ kind: 'profession', name: 'Animal Looter' });
		expect(model.result).toBeNull();
		expect(model.loading).toBe(false);
		expect(errors.error).toBe('backend unreachable');
	});
});

describe('select', () => {
	it('switches the selected candidate and falls back to the top for unknown names', async () => {
		const { model } = makeModel();
		mocked.getActivityRecommender.mockResolvedValue(recommenderResult());
		await model.load({ kind: 'profession', name: 'Animal Looter' });

		model.select('Gardener');
		expect(model.selected?.activity).toBe('Gardener');

		model.select('Not A Candidate');
		expect(model.selected?.activity).toBe('Resource Gatherer');
	});
});
