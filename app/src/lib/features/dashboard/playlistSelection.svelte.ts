/**
 * The dashboard's playlist-selection view model: which playlist the
 * Quests widget is showing, and that playlist's items resolved against
 * live quest state and their cooldowns, split into the two groups the
 * widget renders.
 *
 * The quest data itself belongs to the shared quests feature model; this
 * is only the dashboard's view over it, which is why it lives here
 * rather than beside the quests domain.
 *
 * Guide mode pins the selection to the first demo playlist so the widget
 * shows populated content during the tour, and the pre-guide selection
 * is snapshotted so the tour hands it back afterwards.
 */

import { getCooldownRemaining, getCooldownStatus } from '$lib/features/quests/cooldown';
import type { QuestsModel } from '$lib/features/quests/questsModel.svelte';
import type { CooldownStatus } from '$lib/types/common';
import type { Quest, QuestPlaylist } from '$lib/types/quests';

export interface PlaylistQuestItem {
	quest: Quest;
	description: string | null;
	cd: CooldownStatus;
	inProgress: boolean;
}

export function createPlaylistSelection(quests: QuestsModel, isGuideActive: () => boolean) {
	let activePlaylistId = $state<string | null>(null);
	let now = $state(Date.now());
	// Undefined means "no snapshot held"; null is a valid snapshot.
	let snapshot: string | null | undefined = undefined;

	const activePlaylist = $derived(
		quests.playlists.find((playlist) => playlist.id === activePlaylistId) ?? null,
	);

	function itemsForGroup(groupType: 'immediate' | 'long_horizon'): PlaylistQuestItem[] {
		if (!activePlaylist) return [];
		const out: PlaylistQuestItem[] = [];
		for (const item of activePlaylist.items) {
			if (item.groupType !== groupType) continue;
			const quest = quests.quests.find((q) => q.id === item.questId);
			if (!quest) continue;
			out.push({
				quest,
				description: item.description,
				cd: getCooldownStatus(quest, now),
				inProgress: quest.startedAt != null,
			});
		}
		return out;
	}

	const immediateItems = $derived.by(() => itemsForGroup('immediate'));
	const longHorizonItems = $derived.by(() => itemsForGroup('long_horizon'));

	/** Reconcile the selection with a freshly loaded playlist set: drop a
	 * selection whose playlist is gone, and pin the demo one under the
	 * guide. */
	function sync(loaded: QuestPlaylist[]) {
		if (loaded.length === 0) {
			activePlaylistId = null;
			return;
		}
		if (isGuideActive()) {
			activePlaylistId = loaded[0].id;
			return;
		}
		if (activePlaylistId && !loaded.some((playlist) => playlist.id === activePlaylistId)) {
			activePlaylistId = null;
		}
	}

	/** Hold the pre-guide selection so the tour can hand it back. */
	function snapshotForGuide() {
		if (snapshot === undefined) snapshot = activePlaylistId;
	}

	/** Restore on guide-close, even when the reload failed: a surviving
	 * snapshot would clobber the user's next selection on the following
	 * guide cycle. `sync` then validates the restored id against the
	 * freshly loaded real playlists. */
	function restoreFromGuide() {
		if (snapshot === undefined) return;
		activePlaylistId = snapshot;
		snapshot = undefined;
	}

	function tick() {
		now = Date.now();
	}

	return {
		get activePlaylistId() {
			return activePlaylistId;
		},
		set activePlaylistId(value: string | null) {
			activePlaylistId = value;
		},
		get activePlaylist() {
			return activePlaylist;
		},
		get immediateItems() {
			return immediateItems;
		},
		get longHorizonItems() {
			return longHorizonItems;
		},
		/** Cooldown remaining for a quest, against this model's own clock. */
		cooldownRemaining(quest: Quest) {
			return getCooldownRemaining(quest, now);
		},
		sync,
		snapshotForGuide,
		restoreFromGuide,
		tick,
	};
}

export type PlaylistSelection = ReturnType<typeof createPlaylistSelection>;
