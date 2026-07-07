/**
 * Playlist view model: the playlist form modal (create/edit, group reorder
 * and cross-group moves) and the playlist-list derivations. Depends on the
 * quests model for the quest catalogue, the shared error strip, and the
 * shared menu delete-confirm state.
 */

import { createPlaylist, deletePlaylist, updatePlaylist } from '$lib/api';
import type { PlaylistItemGroup, Quest, QuestPlaylist } from '$lib/types';
import { describeError } from '$lib/view/errorState';
import { getCooldownStatus } from './cooldown';

export interface PlaylistFormItem {
	quest_id: string;
	description: string | null;
	group_type: PlaylistItemGroup;
}

export interface PlaylistFormState {
	name: string;
	planet: string;
	estimated_minutes: number;
	immediate_items: PlaylistFormItem[];
	long_horizon_items: PlaylistFormItem[];
}

function defaultPlaylistForm(): PlaylistFormState {
	return {
		name: '',
		planet: 'Calypso',
		estimated_minutes: 30,
		immediate_items: [],
		long_horizon_items: [],
	};
}

/** The slice of the quests model this model reads and writes. */
export interface PlaylistModelDeps {
	readonly quests: Quest[];
	playlists: QuestPlaylist[];
	error: string | null;
	deleteConfirmId: string | null;
}

export function createPlaylistModel(deps: PlaylistModelDeps) {
	// ── Playlist view state ──
	let expandedPlaylistId = $state<string | null>(null);

	// ── Playlist modal ──
	let showPlaylistModal = $state(false);
	let editingPlaylist = $state<QuestPlaylist | null>(null);
	let playlistForm = $state(defaultPlaylistForm());

	const availableForPlaylist = $derived(
		deps.quests.filter(
			(q) =>
				!playlistForm.immediate_items.some((item) => item.quest_id === q.id) &&
				!playlistForm.long_horizon_items.some((item) => item.quest_id === q.id),
		),
	);

	// ── Computed: playlist with quest data ──
	function playlistQuestItems(pl: QuestPlaylist, groupType?: PlaylistItemGroup) {
		return pl.items
			.filter((item) => !groupType || item.groupType === groupType)
			.map((item) => {
				const quest = deps.quests.find((q) => q.id === item.questId);
				return quest ? { quest, description: item.description, groupType: item.groupType } : null;
			})
			.filter(
				(x): x is { quest: Quest; description: string | null; groupType: PlaylistItemGroup } =>
					x !== null,
			);
	}

	function playlistAllReady(pl: QuestPlaylist, now: number): boolean {
		const immediateItems = playlistQuestItems(pl, 'immediate');
		if (immediateItems.length === 0) return false;
		return immediateItems.every((item) => {
			const s = getCooldownStatus(item.quest, now);
			return s === 'ready' || s === 'no_cooldown';
		});
	}

	// ── Playlist CRUD ──
	function openNewPlaylist() {
		editingPlaylist = null;
		playlistForm = defaultPlaylistForm();
		showPlaylistModal = true;
	}

	function openEditPlaylist(playlist: QuestPlaylist) {
		editingPlaylist = playlist;
		playlistForm = {
			name: playlist.name,
			planet: playlist.planet,
			estimated_minutes: playlist.estimatedMinutes,
			immediate_items: playlist.items
				.filter((item) => item.groupType === 'immediate')
				.map((item) => ({
					quest_id: item.questId,
					description: item.description,
					group_type: 'immediate' as const,
				})),
			long_horizon_items: playlist.items
				.filter((item) => item.groupType === 'long_horizon')
				.map((item) => ({
					quest_id: item.questId,
					description: item.description,
					group_type: 'long_horizon' as const,
				})),
		};
		showPlaylistModal = true;
	}

	async function savePlaylist() {
		const data = {
			name: playlistForm.name,
			planet: playlistForm.planet,
			estimated_minutes: playlistForm.estimated_minutes,
			items: [
				...playlistForm.immediate_items.map((item) => ({
					quest_id: parseInt(item.quest_id, 10),
					description: item.description,
					group_type: 'immediate' as const,
				})),
				...playlistForm.long_horizon_items.map((item) => ({
					quest_id: parseInt(item.quest_id, 10),
					description: item.description,
					group_type: 'long_horizon' as const,
				})),
			],
		};
		try {
			if (editingPlaylist) {
				const updated = await updatePlaylist(editingPlaylist.id, data);
				deps.playlists = deps.playlists.map((p) => (p.id === updated.id ? updated : p));
			} else {
				const created = await createPlaylist(data);
				deps.playlists = [...deps.playlists, created];
			}
			showPlaylistModal = false;
		} catch (e) {
			deps.error = describeError(e, 'Failed to save playlist');
		}
	}

	async function handleDeletePlaylist(playlistId: string) {
		try {
			await deletePlaylist(playlistId);
			deps.playlists = deps.playlists.filter((p) => p.id !== playlistId);
			deps.deleteConfirmId = null;
			if (expandedPlaylistId === playlistId) expandedPlaylistId = null;
		} catch (e) {
			deps.error = describeError(e, 'Failed to delete playlist');
		}
	}

	function questName(id: string): string {
		return deps.quests.find((q) => q.id === id)?.name ?? `Quest #${id}`;
	}

	function moveQuestUp(groupType: PlaylistItemGroup, index: number) {
		if (index === 0) return;
		const items =
			groupType === 'immediate'
				? [...playlistForm.immediate_items]
				: [...playlistForm.long_horizon_items];
		[items[index - 1], items[index]] = [items[index], items[index - 1]];
		if (groupType === 'immediate') playlistForm.immediate_items = items;
		else playlistForm.long_horizon_items = items;
	}

	function moveQuestDown(groupType: PlaylistItemGroup, index: number) {
		const items =
			groupType === 'immediate'
				? [...playlistForm.immediate_items]
				: [...playlistForm.long_horizon_items];
		if (index >= items.length - 1) return;
		[items[index], items[index + 1]] = [items[index + 1], items[index]];
		if (groupType === 'immediate') playlistForm.immediate_items = items;
		else playlistForm.long_horizon_items = items;
	}

	function addQuestToPlaylist(questId: string, groupType: PlaylistItemGroup) {
		const item = { quest_id: questId, description: null, group_type: groupType };
		if (groupType === 'immediate') {
			playlistForm.immediate_items = [...playlistForm.immediate_items, item];
		} else {
			playlistForm.long_horizon_items = [...playlistForm.long_horizon_items, item];
		}
	}

	function removeQuestFromPlaylist(questId: string, groupType: PlaylistItemGroup) {
		if (groupType === 'immediate') {
			playlistForm.immediate_items = playlistForm.immediate_items.filter(
				(item) => item.quest_id !== questId,
			);
		} else {
			playlistForm.long_horizon_items = playlistForm.long_horizon_items.filter(
				(item) => item.quest_id !== questId,
			);
		}
	}

	function moveQuestBetweenGroups(questId: string, sourceGroup: PlaylistItemGroup) {
		const targetGroup = sourceGroup === 'immediate' ? 'long_horizon' : 'immediate';
		const sourceItems =
			sourceGroup === 'immediate' ? playlistForm.immediate_items : playlistForm.long_horizon_items;
		const item = sourceItems.find((entry) => entry.quest_id === questId);
		if (!item) return;
		if (sourceGroup === 'immediate') {
			playlistForm.immediate_items = playlistForm.immediate_items.filter(
				(entry) => entry.quest_id !== questId,
			);
			playlistForm.long_horizon_items = [
				...playlistForm.long_horizon_items,
				{ ...item, group_type: targetGroup },
			];
		} else {
			playlistForm.long_horizon_items = playlistForm.long_horizon_items.filter(
				(entry) => entry.quest_id !== questId,
			);
			playlistForm.immediate_items = [
				...playlistForm.immediate_items,
				{ ...item, group_type: targetGroup },
			];
		}
	}

	return {
		get expandedPlaylistId() {
			return expandedPlaylistId;
		},
		set expandedPlaylistId(value: string | null) {
			expandedPlaylistId = value;
		},
		get showPlaylistModal() {
			return showPlaylistModal;
		},
		set showPlaylistModal(value: boolean) {
			showPlaylistModal = value;
		},
		get editingPlaylist() {
			return editingPlaylist;
		},
		set editingPlaylist(value: QuestPlaylist | null) {
			editingPlaylist = value;
		},
		get playlistForm() {
			return playlistForm;
		},
		get availableForPlaylist() {
			return availableForPlaylist;
		},

		playlistQuestItems,
		playlistAllReady,
		openNewPlaylist,
		openEditPlaylist,
		savePlaylist,
		handleDeletePlaylist,
		questName,
		moveQuestUp,
		moveQuestDown,
		addQuestToPlaylist,
		removeQuestFromPlaylist,
		moveQuestBetweenGroups,
	};
}

export type PlaylistModel = ReturnType<typeof createPlaylistModel>;
