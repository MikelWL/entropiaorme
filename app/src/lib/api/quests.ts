/**
 * The quests family: quest and playlist CRUD, the lifecycle verbs, and
 * the curated analytics rows. Thin wrappers over the generated typed
 * commands; argument shaping (string ids onto numeric commands) only.
 */

import type {
	PlaylistInput,
	Quest,
	QuestFamily,
	QuestFamilyInput,
	QuestInput,
	QuestPlaylist,
} from './commands.gen';
import * as commands from './commands.gen';

export const getQuests = commands.questsList;
export const getQuestAnalytics = commands.questsAnalytics;
export const getPlaylistAnalytics = commands.playlistsAnalytics;
export const getPlaylists = commands.playlistsList;
export const createQuest = commands.questCreate;
export const createPlaylist = commands.playlistCreate;
export const getQuestFamilies = commands.questFamiliesList;
export const createQuestFamily = commands.questFamilyCreate;

export async function updateQuestFamily(id: string, data: QuestFamilyInput): Promise<QuestFamily> {
	return commands.questFamilyUpdate(Number(id), data);
}

export async function deleteQuestFamily(id: string): Promise<void> {
	await commands.questFamilyDelete(Number(id));
}

export async function getQuest(id: string): Promise<Quest> {
	return commands.questGet(Number(id));
}

export async function updateQuest(id: string, data: QuestInput): Promise<Quest> {
	return commands.questUpdate(Number(id), data);
}

export async function deleteQuest(id: string): Promise<void> {
	await commands.questDelete(Number(id));
}

export async function startQuest(id: string): Promise<Quest> {
	return commands.questStart(Number(id));
}

export async function completeQuest(id: string): Promise<Quest> {
	return commands.questComplete(Number(id));
}

export async function cancelQuest(id: string, undoReward = false): Promise<Quest> {
	return commands.questCancel(Number(id), undoReward);
}

export async function updatePlaylist(id: string, data: PlaylistInput): Promise<QuestPlaylist> {
	return commands.playlistUpdate(Number(id), data);
}

export async function deletePlaylist(id: string): Promise<void> {
	await commands.playlistDelete(Number(id));
}
