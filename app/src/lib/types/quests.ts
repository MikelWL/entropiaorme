/**
 * Quest-surface types. The wire shapes re-export from the generated
 * bindings (the authoritative contract); the create/update payload
 * names alias the generated inputs they always mirrored.
 */

export type {
	PlaylistAnalyticsRow,
	PlaylistInput as PlaylistCreateData,
	PlaylistInput as PlaylistUpdateData,
	PlaylistItem,
	PlaylistItemGroup,
	Quest,
	QuestAnalyticsRow,
	QuestCooldownAnchor,
	QuestFamily,
	QuestFamilyInput as QuestFamilyCreateData,
	QuestFamilyInput as QuestFamilyUpdateData,
	QuestInput as QuestCreateData,
	QuestInput as QuestUpdateData,
	QuestPlaylist,
} from '$lib/api/commands.gen';
