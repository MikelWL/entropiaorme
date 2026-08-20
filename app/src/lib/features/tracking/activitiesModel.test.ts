import { describe, expect, it, vi } from 'vitest';
import type { ActivityOption, ActivityOptionsResult } from '$lib/api';
import { ApiError } from '$lib/api';
import { type ActivitiesModelDeps, createActivitiesModel } from './activitiesModel.svelte';

function option(overrides: Partial<ActivityOption> = {}): ActivityOption {
	return {
		key: 'quest:11',
		kind: 'quest',
		name: 'Daily: Carabok',
		questId: 11,
		active: false,
		available: true,
		resettable: false,
		unavailableReason: null,
		availableFrom: null,
		offRoster: false,
		manualHandIn: false,
		handInWaiting: false,
		rosterOrder: 0,
		...overrides,
	};
}

function harness(overrides: Partial<ActivitiesModelDeps> = {}) {
	const offerings: ActivityOptionsResult = {
		definitionId: 1,
		definitionName: 'Daily Hunt',
		visible: true,
		adHocSegments: true,
		readyCount: 1,
		options: [option()],
		active: [],
	};
	const deps: ActivitiesModelDeps = {
		readOptions: vi.fn(async () => offerings),
		activateQuest: vi.fn(async () => {}),
		activateSegment: vi.fn(async () => {}),
		deactivateQuest: vi.fn(async () => {}),
		deactivateSegment: vi.fn(async () => {}),
		beginHandIn: vi.fn(async () => ({
			questId: 11,
			questName: 'Daily: Carabok',
			waiting: false,
			candidate: null,
		})),
		refresh: vi.fn(async () => {}),
		...overrides,
	};
	return { model: createActivitiesModel(deps), deps, offerings };
}

describe('reading the offerings', () => {
	it('answers with what it fetched, so the caller can size the menu in one motion', async () => {
		const { model, offerings } = harness();

		const loaded = await model.load();

		expect(loaded).toBe(offerings);
		expect(model.options).toBe(offerings);
		expect(model.error).toBeNull();
	});

	it('surfaces a failed read instead of leaving the control blank and silent', async () => {
		const { model } = harness({
			readOptions: vi.fn(async () => {
				throw new ApiError('badRequest', 'Nope');
			}),
		});

		expect(await model.load()).toBeNull();
		expect(model.error).toBe('Nope');
	});

	it('clears a prior failure once a read succeeds', async () => {
		const readOptions = vi
			.fn<ActivitiesModelDeps['readOptions']>()
			.mockRejectedValueOnce(new ApiError('badRequest', 'Nope'));
		const { model, offerings } = harness({ readOptions });
		readOptions.mockResolvedValue(offerings);

		await model.load();
		await model.load();

		expect(model.error).toBeNull();
	});
});

describe('declaring an activity', () => {
	it('declares a quest row exclusively on a tap, and refreshes both readouts', async () => {
		const { model, deps } = harness();

		await model.toggle(option());

		expect(deps.activateQuest).toHaveBeenCalledWith(11, false);
		expect(deps.refresh).toHaveBeenCalled();
		expect(deps.readOptions).toHaveBeenCalled();
	});

	it('co-activates rather than switching when asked to', async () => {
		const { model, deps } = harness();

		await model.declare(option(), true);

		expect(deps.activateQuest).toHaveBeenCalledWith(11, true);
	});

	it('ends a standing row on a tap, which is the only way to record nothing in particular', async () => {
		const { model, deps } = harness();

		await model.toggle(option({ active: true }));

		expect(deps.deactivateQuest).toHaveBeenCalledWith(11);
		expect(deps.activateQuest).not.toHaveBeenCalled();
	});

	it('matches a standing segment by its name, which is all such a slice has', async () => {
		const { model, deps } = harness();

		await model.toggle(
			option({
				key: 'segment:Warm-up',
				kind: 'segment',
				name: 'Warm-up',
				questId: null,
				active: true,
			}),
		);

		expect(deps.deactivateSegment).toHaveBeenCalledWith('Warm-up');
	});

	it('declares a segment row under its authored name', async () => {
		const { model, deps } = harness();

		await model.toggle(
			option({ key: 'segment:Warm-up', kind: 'segment', name: 'Warm-up', questId: null }),
		);

		expect(deps.activateSegment).toHaveBeenCalledWith('Warm-up', false);
	});

	it('acts on the variant a family row resolved to, not on the family', async () => {
		const { model, deps } = harness();

		await model.toggle(
			option({
				key: 'quest_family:3',
				kind: 'quest_family',
				name: 'ARIS - Daily Hunting 1: Weak Mortirex',
				questId: 42,
			}),
		);

		expect(deps.activateQuest).toHaveBeenCalledWith(42, false);
	});

	it('declares nothing for a family row with no variant in play', async () => {
		const { model, deps } = harness();

		expect(
			await model.toggle(
				option({
					key: 'quest_family:3',
					kind: 'quest_family',
					name: 'ARIS - Daily Hunting 1',
					questId: null,
					available: false,
				}),
			),
		).toBe(false);
		expect(deps.activateQuest).not.toHaveBeenCalled();
	});

	it('surfaces a refused declaration instead of swallowing it', async () => {
		const { model } = harness({
			activateQuest: vi.fn(async () => {
				throw new ApiError('badRequest', 'Quest is not in progress');
			}),
		});

		expect(await model.toggle(option())).toBe(false);
		expect(model.error).toBe('Quest is not in progress');
	});
});

describe('naming an activity in play', () => {
	it('sends the trimmed name and clears the field, since it is now a row', async () => {
		const { model, deps } = harness();
		model.segmentDraft = '  Boss lap  ';

		await model.declareTyped();

		expect(deps.activateSegment).toHaveBeenCalledWith('Boss lap', false);
		expect(model.segmentDraft).toBe('');
	});

	it('declares nothing from a blank draft: there is no unnamed slice', async () => {
		const { model, deps } = harness();

		expect(await model.declareTyped()).toBe(false);
		expect(deps.activateSegment).not.toHaveBeenCalled();
	});

	it('keeps the typed name when the write is refused, so it is not lost', async () => {
		const { model } = harness({
			activateSegment: vi.fn(async () => {
				throw new ApiError('conflict', 'No active session');
			}),
		});
		model.segmentDraft = 'Boss lap';

		await model.declareTyped();

		expect(model.segmentDraft).toBe('Boss lap');
		expect(model.error).toBe('No active session');
	});
});

describe('reviewing a manual hand-in', () => {
	it('begins the exact-clump review and refreshes the parent snapshot', async () => {
		const { model, deps } = harness();

		const handIn = await model.beginHandIn(option({ manualHandIn: true }));

		expect(handIn?.questId).toBe(11);
		expect(deps.beginHandIn).toHaveBeenCalledWith(11);
		expect(deps.refresh).toHaveBeenCalledOnce();
	});
});
