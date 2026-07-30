import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Quest, QuestPlaylist } from '$lib/types';
import { createPlaylistModel, type PlaylistModelDeps } from './playlistModel.svelte';

vi.mock('$lib/api', () => ({
	createPlaylist: vi.fn(),
	updatePlaylist: vi.fn(),
	deletePlaylist: vi.fn(),
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
		expectedRewardMarkupPercent: null,
		rewardDescription: '',
		notes: '',
		chainName: null,
		chainPosition: null,
		chainTotal: null,
		playlistIds: [],
		startedAt: null,
		signalLootItem: null,
		...overrides,
	};
}

function playlist(overrides: Partial<QuestPlaylist> = {}): QuestPlaylist {
	return {
		id: '10',
		name: 'Daily run',
		planet: 'Calypso',
		estimatedMinutes: 30,
		questIds: ['1', '2'],
		immediateQuestIds: ['1'],
		longHorizonQuestIds: ['2'],
		items: [
			{ questId: '1', description: 'east field', groupType: 'immediate' },
			{ questId: '2', description: null, groupType: 'long_horizon' },
		],
		...overrides,
	};
}

/** A plain deps double standing in for the quests model slice. */
function makeDeps(overrides: Partial<PlaylistModelDeps> = {}): PlaylistModelDeps {
	return {
		quests: [
			quest({ id: '1' }),
			quest({ id: '2', name: 'Atrox Iron' }),
			quest({ id: '3', name: 'Oratan Push' }),
		],
		playlists: [playlist()],
		error: null,
		deleteConfirmId: null,
		...overrides,
	};
}

const NOW = Date.parse('2026-07-07T12:00:00Z');

beforeEach(() => {
	vi.clearAllMocks();
});

describe('playlistQuestItems', () => {
	it('resolves items to quests, filtered by group', () => {
		const model = createPlaylistModel(makeDeps());
		const immediate = model.playlistQuestItems(playlist(), 'immediate');
		expect(immediate.map((i) => i.quest.id)).toEqual(['1']);
		expect(immediate[0].description).toBe('east field');
		const all = model.playlistQuestItems(playlist());
		expect(all.map((i) => i.quest.id)).toEqual(['1', '2']);
	});

	it('drops items whose quest no longer exists', () => {
		const model = createPlaylistModel(makeDeps({ quests: [quest({ id: '2' })] }));
		const all = model.playlistQuestItems(playlist());
		expect(all.map((i) => i.quest.id)).toEqual(['2']);
	});
});

describe('playlistAllReady', () => {
	it('is true when every immediate quest is ready or has no cooldown', () => {
		const deps = makeDeps({
			quests: [
				quest({ id: '1', cooldownDurationHours: null }),
				quest({ id: '2', cooldownExpiresAt: new Date(NOW - 1000).toISOString() }),
			],
		});
		const model = createPlaylistModel(deps);
		const pl = playlist({
			items: [
				{ questId: '1', description: null, groupType: 'immediate' },
				{ questId: '2', description: null, groupType: 'immediate' },
			],
		});
		expect(model.playlistAllReady(pl, NOW)).toBe(true);
	});

	it('is false while any immediate quest is cooling, ignoring long-horizon items', () => {
		const deps = makeDeps({
			quests: [
				quest({ id: '1', cooldownExpiresAt: new Date(NOW + 3600_000).toISOString() }),
				quest({ id: '2' }),
			],
		});
		const model = createPlaylistModel(deps);
		const cooling = playlist({
			items: [
				{ questId: '1', description: null, groupType: 'immediate' },
				{ questId: '2', description: null, groupType: 'long_horizon' },
			],
		});
		expect(model.playlistAllReady(cooling, NOW)).toBe(false);
	});

	it('is false with no immediate items at all', () => {
		const model = createPlaylistModel(makeDeps());
		const empty = playlist({
			items: [{ questId: '2', description: null, groupType: 'long_horizon' }],
		});
		expect(model.playlistAllReady(empty, NOW)).toBe(false);
	});
});

describe('availableForPlaylist', () => {
	it('excludes quests already in either form group', () => {
		const model = createPlaylistModel(makeDeps());
		model.openNewPlaylist();
		expect(model.availableForPlaylist.map((q) => q.id)).toEqual(['1', '2', '3']);
		model.addQuestToPlaylist('1', 'immediate');
		model.addQuestToPlaylist('3', 'long_horizon');
		expect(model.availableForPlaylist.map((q) => q.id)).toEqual(['2']);
	});
});

describe('form reorder and group moves', () => {
	function seededModel() {
		const model = createPlaylistModel(makeDeps());
		model.openNewPlaylist();
		model.addQuestToPlaylist('1', 'immediate');
		model.addQuestToPlaylist('2', 'immediate');
		model.addQuestToPlaylist('3', 'immediate');
		return model;
	}

	const ids = (items: { quest_id: string }[]) => items.map((i) => i.quest_id);

	it('moves an item up and clamps at the top', () => {
		const model = seededModel();
		model.moveQuestUp('immediate', 2);
		expect(ids(model.playlistForm.immediate_items)).toEqual(['1', '3', '2']);
		model.moveQuestUp('immediate', 0);
		expect(ids(model.playlistForm.immediate_items)).toEqual(['1', '3', '2']);
	});

	it('moves an item down and clamps at the bottom', () => {
		const model = seededModel();
		model.moveQuestDown('immediate', 0);
		expect(ids(model.playlistForm.immediate_items)).toEqual(['2', '1', '3']);
		model.moveQuestDown('immediate', 2);
		expect(ids(model.playlistForm.immediate_items)).toEqual(['2', '1', '3']);
	});

	it('moves an item between groups, rewriting its group_type and appending', () => {
		const model = seededModel();
		model.moveQuestBetweenGroups('2', 'immediate');
		expect(ids(model.playlistForm.immediate_items)).toEqual(['1', '3']);
		expect(model.playlistForm.long_horizon_items).toEqual([
			{ quest_id: '2', description: null, group_type: 'long_horizon' },
		]);
		model.moveQuestBetweenGroups('2', 'long_horizon');
		expect(ids(model.playlistForm.immediate_items)).toEqual(['1', '3', '2']);
		expect(model.playlistForm.long_horizon_items).toEqual([]);
	});

	it('removes an item from the named group only', () => {
		const model = seededModel();
		model.addQuestToPlaylist('3', 'long_horizon');
		model.removeQuestFromPlaylist('3', 'immediate');
		expect(ids(model.playlistForm.immediate_items)).toEqual(['1', '2']);
		expect(ids(model.playlistForm.long_horizon_items)).toEqual(['3']);
	});
});

describe('openEditPlaylist', () => {
	it('splits the playlist items into the two form groups', () => {
		const model = createPlaylistModel(makeDeps());
		model.openEditPlaylist(playlist());
		expect(model.editingPlaylist?.id).toBe('10');
		expect(model.playlistForm.immediate_items).toEqual([
			{ quest_id: '1', description: 'east field', group_type: 'immediate' },
		]);
		expect(model.playlistForm.long_horizon_items).toEqual([
			{ quest_id: '2', description: null, group_type: 'long_horizon' },
		]);
		expect(model.showPlaylistModal).toBe(true);
	});
});

describe('savePlaylist', () => {
	it('sends numeric quest ids with group types on create and appends the result', async () => {
		const deps = makeDeps({ playlists: [] });
		const model = createPlaylistModel(deps);
		model.openNewPlaylist();
		model.playlistForm.name = 'Run';
		model.addQuestToPlaylist('2', 'immediate');
		model.addQuestToPlaylist('3', 'long_horizon');
		mocked.createPlaylist.mockResolvedValue(playlist({ id: '11', name: 'Run' }));

		await model.savePlaylist();
		expect(mocked.createPlaylist).toHaveBeenCalledWith(
			expect.objectContaining({
				name: 'Run',
				items: [
					{ quest_id: 2, description: null, group_type: 'immediate' },
					{ quest_id: 3, description: null, group_type: 'long_horizon' },
				],
			}),
		);
		expect(deps.playlists.map((p) => p.id)).toEqual(['11']);
		expect(model.showPlaylistModal).toBe(false);
	});

	it('routes edits through updatePlaylist and swaps the row in place', async () => {
		const deps = makeDeps();
		const model = createPlaylistModel(deps);
		model.openEditPlaylist(deps.playlists[0]);
		model.playlistForm.name = 'Renamed';
		mocked.updatePlaylist.mockResolvedValue(playlist({ name: 'Renamed' }));

		await model.savePlaylist();
		expect(mocked.updatePlaylist).toHaveBeenCalledWith(
			'10',
			expect.objectContaining({ name: 'Renamed' }),
		);
		expect(deps.playlists[0].name).toBe('Renamed');
	});

	it('keeps the modal open and surfaces the message on failure', async () => {
		const deps = makeDeps();
		const model = createPlaylistModel(deps);
		model.openNewPlaylist();
		mocked.createPlaylist.mockRejectedValue(new Error('name required'));
		await model.savePlaylist();
		expect(model.showPlaylistModal).toBe(true);
		expect(deps.error).toBe('name required');
	});
});

describe('handleDeletePlaylist', () => {
	it('removes the playlist, clears delete-confirm and collapses its expansion', async () => {
		const deps = makeDeps({ deleteConfirmId: 'pl-10' });
		const model = createPlaylistModel(deps);
		model.expandedPlaylistId = '10';
		mocked.deletePlaylist.mockResolvedValue(undefined);

		await model.handleDeletePlaylist('10');
		expect(deps.playlists).toEqual([]);
		expect(deps.deleteConfirmId).toBeNull();
		expect(model.expandedPlaylistId).toBeNull();
	});

	it('surfaces a delete failure through the shared error strip', async () => {
		const deps = makeDeps();
		const model = createPlaylistModel(deps);
		mocked.deletePlaylist.mockRejectedValue(new Error('in use'));
		await model.handleDeletePlaylist('10');
		expect(deps.error).toBe('in use');
		expect(deps.playlists).toHaveLength(1);
	});
});

describe('questName', () => {
	it('resolves a known id and falls back to a labelled placeholder', () => {
		const model = createPlaylistModel(makeDeps());
		expect(model.questName('2')).toBe('Atrox Iron');
		expect(model.questName('99')).toBe('Quest #99');
	});
});
