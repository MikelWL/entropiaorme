/**
 * Quest-surface types. The wire shapes re-export from the generated
 * bindings (the authoritative contract); the create/update payload
 * names alias the generated inputs they always mirrored.
 */

export type {
	Quest,
	QuestAnalyticsRow,
	QuestCompletionTrigger,
	QuestCooldownAnchor,
	QuestFamily,
	QuestFamilyInput as QuestFamilyCreateData,
	QuestFamilyInput as QuestFamilyUpdateData,
	QuestHandInCandidate,
	QuestHandInItem,
	QuestHandInState,
	QuestInput as QuestCreateData,
	QuestInput as QuestUpdateData,
	QuestRewardPolicy,
} from '$lib/api/commands.gen';
