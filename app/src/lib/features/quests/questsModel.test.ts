import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Quest } from '$lib/types';
import { createQuestsModel } from './questsModel.svelte';

vi.mock('$lib/api', () => ({
	getQuests: vi.fn(),
	getPlaylists: vi.fn(),
	getQuestFamilies: vi.fn(),
	createQuest: vi.fn(),
	updateQuest: vi.fn(),
	deleteQuest: vi.fn(),
	startQuest: vi.fn(),
	completeQuest: vi.fn(),
	cancelQuest: vi.fn(),
	getQuestAnalytics: vi.fn(),
	getPlaylistAnalytics: vi.fn(),
	getAnalyticsOverview: vi.fn(),
}));

import * as api from '$lib/api';

const mocked = vi.mocked(api);

function quest(overrides: Partial<Quest> = {}): Quest {
	return {
		id: '1',
		name: 'Daily Kill - Caboria',
		category: null,
		targetMobs: ['Caboria'],
		planet: 'Calypso',
		waypoint: null,
		cooldownDurationHours: 21,
		cooldownExpiresAt: null,
		reward: 1.2,
		rewardIsSkill: false,
		expectedRewardMarkupPercent: 130,
		rewardDescription: '',
		notes: '',
		chainName: null,
		chainPosition: null,
		chainTotal: null,
		playlistIds: [],
		startedAt: null,
		signalLootItem: null,
		cooldownAnchor: 'completion',
		lastStartedAt: null,
		familyId: null,
		familyName: null,
		familyCooldownDurationHours: null,
		familyCooldownAnchor: null,
		familyCooldownExpiresAt: null,
		...overrides,
	};
}

const NOW = Date.parse('2026-07-07T12:00:00Z');

beforeEach(() => {
	vi.clearAllMocks();
	mocked.getQuests.mockResolvedValue([]);
	mocked.getPlaylists.mockResolvedValue([]);
	mocked.getQuestFamilies.mockResolvedValue([]);
});

describe('loadData', () => {
	it('loads quests and playlists and collapses every category once', async () => {
		const rows = [
			quest({ id: '1', category: 'Iron' }),
			quest({ id: '2', category: 'Daily' }),
			quest({ id: '3', category: null }),
		];
		mocked.getQuests.mockResolvedValue(rows);
		const model = createQuestsModel();
		await model.loadData(false);

		expect(model.loading).toBe(false);
		expect(model.quests).toHaveLength(3);
		expect([...model.collapsedCategories].sort()).toEqual(['Daily', 'Iron']);
	});

	it('does not re-collapse categories the user has expanded on a later load', async () => {
		mocked.getQuests.mockResolvedValue([quest({ id: '1', category: 'Iron' })]);
		const model = createQuestsModel();
		await model.loadData(false);

		model.collapsedCategories = new Set();
		await model.loadData(false);
		expect(model.collapsedCategories.size).toBe(0);
	});

	it('surfaces a load failure through the error strip', async () => {
		mocked.getQuests.mockRejectedValue(new Error('backend unreachable'));
		const model = createQuestsModel();
		await model.loadData(false);
		expect(model.error).toBe('backend unreachable');
		expect(model.loading).toBe(false);
	});

	it('seeds the guide fixtures without touching the API in guide mode', async () => {
		const model = createQuestsModel();
		await model.loadData(true);
		expect(mocked.getQuests).not.toHaveBeenCalled();
		expect(model.quests.length).toBeGreaterThan(0);
		expect(model.analyticsLoaded).toBe(true);
		expect(model.rates.liquidReturnRate).toBeGreaterThan(0);
	});
});

describe('refresh', () => {
	it('replaces quests and playlists on success', async () => {
		const model = createQuestsModel();
		mocked.getQuests.mockResolvedValue([quest({ id: '9' })]);
		await model.refresh();
		expect(model.quests.map((q) => q.id)).toEqual(['9']);
	});

	it('keeps the last good data and stays silent when the poll tick fails', async () => {
		mocked.getQuests.mockResolvedValue([quest({ id: '1' })]);
		const model = createQuestsModel();
		await model.loadData(false);

		mocked.getQuests.mockRejectedValue(new Error('transient'));
		await expect(model.refresh()).resolves.toBeUndefined();
		expect(model.quests.map((q) => q.id)).toEqual(['1']);
		expect(model.error).toBeNull();
	});

	it('drops a pending cancel choice whose quest vanished from the refreshed list', async () => {
		mocked.getQuests.mockResolvedValue([quest({ id: '1' }), quest({ id: '2' })]);
		const model = createQuestsModel();
		await model.loadData(false);
		model.toggleCancelChoice('2');
		expect(model.pendingCancelChoiceQuestId).toBe('2');

		mocked.getQuests.mockResolvedValue([quest({ id: '1' })]);
		await model.refresh();
		expect(model.pendingCancelChoiceQuestId).toBeNull();
	});

	it('keeps a pending cancel choice whose quest survives the refresh', async () => {
		mocked.getQuests.mockResolvedValue([quest({ id: '1' })]);
		const model = createQuestsModel();
		await model.loadData(false);
		model.toggleCancelChoice('1');

		await model.refresh();
		expect(model.pendingCancelChoiceQuestId).toBe('1');
	});
});

describe('filtering', () => {
	async function loadedModel() {
		mocked.getQuests.mockResolvedValue([
			quest({
				id: '1',
				name: 'Atrox Iron',
				planet: 'Calypso',
				targetMobs: ['Atrox'],
				category: 'Iron',
			}),
			quest({
				id: '2',
				name: 'Caboria Daily',
				planet: 'Calypso',
				targetMobs: ['Caboria'],
				category: 'Daily',
			}),
			quest({
				id: '3',
				name: 'Oratan Push',
				planet: 'Arkadia',
				targetMobs: ['Oratan'],
				category: null,
			}),
		]);
		const model = createQuestsModel();
		await model.loadData(false);
		return model;
	}

	it('derives sorted planets and planet-scoped mobs', async () => {
		const model = await loadedModel();
		expect(model.planets).toEqual(['Arkadia', 'Calypso']);
		expect(model.mobs).toEqual(['Atrox', 'Caboria', 'Oratan']);
		model.selectedPlanet = 'Calypso';
		expect(model.mobs).toEqual(['Atrox', 'Caboria']);
	});

	it('stacks planet, mob and search filters', async () => {
		const model = await loadedModel();
		model.selectedPlanet = 'Calypso';
		model.selectedMob = 'Caboria';
		expect(model.filteredQuests.map((q) => q.id)).toEqual(['2']);
		model.selectedMob = null;
		model.searchQuery = 'atrox';
		expect(model.filteredQuests.map((q) => q.id)).toEqual(['1']);
	});

	it('matches the search against name, mob, planet and category', async () => {
		const model = await loadedModel();
		model.searchQuery = 'arkadia';
		expect(model.filteredQuests.map((q) => q.id)).toEqual(['3']);
		model.searchQuery = 'daily';
		expect(model.filteredQuests.map((q) => q.id)).toEqual(['2']);
	});

	it('groups uncategorised quests first, then categories in encounter order', async () => {
		const model = await loadedModel();
		expect(model.questsByCategory.map((g) => g.category)).toEqual(['', 'Iron', 'Daily']);
		expect(model.questsByCategory[0].quests.map((q) => q.id)).toEqual(['3']);
	});
});

describe('categoryStatusCounts', () => {
	it('classifies started, cooling and ready quests', () => {
		const model = createQuestsModel();
		const rows = [
			quest({ id: '1', startedAt: NOW / 1000 }),
			quest({ id: '2', cooldownExpiresAt: new Date(NOW + 3600_000).toISOString() }),
			quest({ id: '3' }),
			quest({ id: '4', cooldownDurationHours: null }),
		];
		expect(model.categoryStatusCounts(rows, NOW)).toEqual({ ready: 2, started: 1, cooling: 1 });
	});
});

describe('quest lifecycle', () => {
	it('replaces the started quest in place and clears its pending cancel choice', async () => {
		mocked.getQuests.mockResolvedValue([quest({ id: '1' }), quest({ id: '2' })]);
		const model = createQuestsModel();
		await model.loadData(false);
		model.toggleCancelChoice('1');
		expect(model.pendingCancelChoiceQuestId).toBe('1');

		mocked.startQuest.mockResolvedValue(quest({ id: '1', startedAt: 123 }));
		await model.handleStart('1');
		expect(model.quests.find((q) => q.id === '1')?.startedAt).toBe(123);
		expect(model.pendingCancelChoiceQuestId).toBeNull();
	});

	it('leaves the pending cancel choice of another quest alone', async () => {
		mocked.getQuests.mockResolvedValue([quest({ id: '1' }), quest({ id: '2' })]);
		const model = createQuestsModel();
		await model.loadData(false);
		model.toggleCancelChoice('2');

		mocked.completeQuest.mockResolvedValue(quest({ id: '1' }));
		await model.handleComplete('1');
		expect(model.pendingCancelChoiceQuestId).toBe('2');
	});

	it('passes the undo-reward flag through to cancel', async () => {
		mocked.cancelQuest.mockResolvedValue(quest({ id: '1' }));
		const model = createQuestsModel();
		await model.handleCancel('1', true);
		expect(mocked.cancelQuest).toHaveBeenCalledWith('1', true);
	});

	it('surfaces a lifecycle failure through the error strip', async () => {
		mocked.startQuest.mockRejectedValue(new Error('cooldown active'));
		const model = createQuestsModel();
		await model.handleStart('1');
		expect(model.error).toBe('cooldown active');
	});

	it('clears a stale error when a later action succeeds', async () => {
		mocked.startQuest.mockRejectedValueOnce(new Error('cooldown active'));
		const model = createQuestsModel();
		await model.handleStart('1');
		expect(model.error).toBe('cooldown active');

		mocked.completeQuest.mockResolvedValue(quest({ id: '1' }));
		await model.handleComplete('1');
		expect(model.error).toBeNull();
	});
});

describe('guide mode', () => {
	it('re-arms the lazy analytics load when leaving guide mode', async () => {
		const model = createQuestsModel();
		await model.loadData(true);
		expect(model.analyticsLoaded).toBe(true);

		mocked.getQuests.mockResolvedValue([]);
		mocked.getPlaylists.mockResolvedValue([]);
		await model.loadData(false);
		expect(model.analyticsLoaded).toBe(false);
	});
});

describe('quest form', () => {
	it('openEditQuest derives days when the cooldown is a whole-day multiple', () => {
		const model = createQuestsModel();
		model.openEditQuest(quest({ cooldownDurationHours: 48 }));
		expect(model.cooldownUnit).toBe('days');
		expect(model.cooldownInput).toBe(2);
	});

	it('openEditQuest stays in hours otherwise', () => {
		const model = createQuestsModel();
		model.openEditQuest(quest({ cooldownDurationHours: 30 }));
		expect(model.cooldownUnit).toBe('hours');
		expect(model.cooldownInput).toBe(30);
	});

	it('saveQuest converts a day-denominated cooldown to hours', async () => {
		mocked.createQuest.mockResolvedValue(quest({ id: '7' }));
		const model = createQuestsModel();
		model.openNewQuest();
		model.questForm.name = 'Weekly';
		model.cooldownUnit = 'days';
		model.cooldownInput = 7;
		await model.saveQuest();
		expect(mocked.createQuest).toHaveBeenCalledWith(
			expect.objectContaining({ cooldown_hours: 168 }),
		);
		expect(model.showQuestModal).toBe(false);
		expect(model.quests.map((q) => q.id)).toContain('7');
	});

	it('saveQuest sends the cooldown unchanged in hours mode and null when unset', async () => {
		mocked.createQuest.mockResolvedValue(quest({ id: '7' }));
		const model = createQuestsModel();
		model.openNewQuest();
		model.questForm.name = 'Daily';
		model.cooldownInput = 21;
		await model.saveQuest();
		expect(mocked.createQuest).toHaveBeenCalledWith(
			expect.objectContaining({ cooldown_hours: 21 }),
		);

		model.openNewQuest();
		model.questForm.name = 'No CD';
		await model.saveQuest();
		expect(mocked.createQuest).toHaveBeenLastCalledWith(
			expect.objectContaining({ cooldown_hours: null }),
		);
	});

	it('saveQuest strips the reward markup for skill quests and zero rewards', async () => {
		mocked.createQuest.mockResolvedValue(quest({ id: '7' }));
		const model = createQuestsModel();
		model.openNewQuest();
		model.questForm.name = 'Skill';
		model.questForm.reward_ped = 5;
		model.questForm.reward_is_skill = true;
		model.questForm.expected_reward_markup_percent = 130;
		await model.saveQuest();
		expect(mocked.createQuest).toHaveBeenCalledWith(
			expect.objectContaining({ expected_reward_markup_percent: null }),
		);
	});

	it('saveQuest routes edits through updateQuest and swaps the row in place', async () => {
		mocked.getQuests.mockResolvedValue([quest({ id: '1', name: 'Old' })]);
		const model = createQuestsModel();
		await model.loadData(false);
		model.openEditQuest(model.quests[0]);
		model.questForm.name = 'New';
		mocked.updateQuest.mockResolvedValue(quest({ id: '1', name: 'New' }));
		await model.saveQuest();
		expect(mocked.updateQuest).toHaveBeenCalledWith('1', expect.objectContaining({ name: 'New' }));
		expect(model.quests[0].name).toBe('New');
	});

	it('keeps the modal open and surfaces the message when saving fails', async () => {
		mocked.createQuest.mockRejectedValue(new Error('name taken'));
		const model = createQuestsModel();
		model.openNewQuest();
		await model.saveQuest();
		expect(model.showQuestModal).toBe(true);
		expect(model.error).toBe('name taken');
	});

	it('adds trimmed, de-duplicated mobs and clears the input', () => {
		const model = createQuestsModel();
		model.openNewQuest();
		model.mobInput = '  Atrox  ';
		model.addMob();
		expect(model.questForm.mobs).toEqual(['Atrox']);
		expect(model.mobInput).toBe('');
		model.mobInput = 'Atrox';
		model.addMob();
		expect(model.questForm.mobs).toEqual(['Atrox']);
		model.removeMob('Atrox');
		expect(model.questForm.mobs).toEqual([]);
	});
});

describe('quest deletion', () => {
	it('removes the quest and clears the delete-confirm state', async () => {
		mocked.getQuests.mockResolvedValue([quest({ id: '1' })]);
		mocked.deleteQuest.mockResolvedValue(undefined);
		const model = createQuestsModel();
		await model.loadData(false);
		model.deleteConfirmId = '1';
		await model.handleDeleteQuest('1');
		expect(model.quests).toEqual([]);
		expect(model.deleteConfirmId).toBeNull();
	});
});
