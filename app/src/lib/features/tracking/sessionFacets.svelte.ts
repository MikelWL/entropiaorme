/**
 * The session-facet view model: the independent, co-recorded attributions
 * a tracking session carries (designated name, skill boost, declared
 * quest) and the writes that move them.
 *
 * The facets are independent by construction, so each control writes only
 * its own value and carries the others through unchanged. Two rules the
 * backend enforces and this model respects rather than duplicates: the
 * name is fixed while a session runs (a change is refused; correction is
 * a post-hoc rename on the session record) while the boost stays
 * editable throughout, and a quest can only be declared against an
 * active session.
 *
 * The satellite-window plumbing (anchors, popup lifecycle) stays with the
 * overlay route; this model owns the state, the lookups, and the writes,
 * and calls back into the route to present or dismiss the name menu.
 */

import { ApiError } from '$lib/api';
import { createTypeahead } from '$lib/view/typeahead.svelte';

/** One option in the quest-declaration menu. */
export interface QuestOption {
	id: number;
	name: string;
	isPlaylist: boolean;
}

export interface SessionFacetsDeps {
	/** The facets currently in force, as the snapshot reports them. */
	readFacets: () => { name: string | null; boost: number | null };
	/** Whether a session is running (the quest declaration needs one). */
	isSessionActive: () => boolean;
	/** Re-read the snapshot after a successful write. */
	refresh: () => Promise<unknown>;
	/** Prior session names matching a query, most-used first. */
	searchNames: (query: string) => Promise<string[]>;
	/** Full-state facet write: a null clears its facet. */
	setSessionConfig: (name: string | null, boost: number | null) => Promise<unknown>;
	/** Bind (or, with both ids null, clear) the session's quest facet. */
	declareQuest: (questId: number | null, playlistId: number | null) => Promise<unknown>;
	/** The active quests and playlists offered by the picker. */
	listQuests: () => Promise<{ id: string; name: string }[]>;
	listPlaylists: () => Promise<{ id: string; name: string }[]>;
	/** Present or dismiss the name-suggestion menu (route-owned). */
	openNameMenu: () => void | Promise<void>;
	closeNameMenu: () => void | Promise<void>;
}

function describe(error: unknown, fallback: string): string {
	return error instanceof ApiError || error instanceof Error ? error.message : fallback;
}

export function createSessionFacets(deps: SessionFacetsDeps) {
	let nameQuery = $state('');
	let nameInput: HTMLInputElement | null = $state(null);
	let nameInputFocused = $state(false);
	let nameCloseTimer: ReturnType<typeof setTimeout> | undefined;
	let nameError = $state<string | null>(null);
	let savingName = $state(false);

	// The boost's edit buffer. The boost edits live: a pill expiring is a
	// genuine change worth recording, and the session keeps the latest
	// declaration.
	let boostDraft = $state('');
	let savingBoost = $state(false);

	let questSaving = $state(false);
	let questOptions = $state<QuestOption[]>([]);

	// One channel for every facet write failure, so a refusal is surfaced
	// beside the controls rather than swallowed.
	let facetError = $state<string | null>(null);

	const nameTypeahead = createTypeahead<string>({
		search: async (query) => {
			try {
				return await deps.searchNames(query);
			} catch (error) {
				throw new Error(describe(error, 'Name lookup failed'));
			}
		},
		debounceMs: 120,
		minLength: 1,
	});

	function clearNameCloseTimer() {
		if (!nameCloseTimer) return;
		clearTimeout(nameCloseTimer);
		nameCloseTimer = undefined;
	}

	/** The boost currently in force, as a write wants it. Reading the
	 * snapshot (not the draft) keeps a name write from moving the boost as
	 * a side effect. */
	function currentBoost(): number | null {
		const value = deps.readFacets().boost;
		return value && value > 0 ? value : null;
	}

	async function write(name: string | null, boost: number | null) {
		await deps.setSessionConfig(name, boost);
		await deps.refresh();
	}

	async function applyName(name: string) {
		if (!name) return;
		clearNameCloseTimer();
		savingName = true;
		facetError = null;
		try {
			await write(name, currentBoost());
			nameQuery = '';
			nameTypeahead.cancel();
			await deps.closeNameMenu();
		} catch (error) {
			facetError = describe(error, 'Failed to set session name');
		}
		savingName = false;
	}

	async function clearName() {
		savingName = true;
		facetError = null;
		try {
			await write(null, currentBoost());
			nameQuery = '';
			nameTypeahead.cancel();
			await deps.closeNameMenu();
		} catch (error) {
			facetError = describe(error, 'Failed to clear session name');
		}
		savingName = false;
	}

	async function commitBoost() {
		const trimmed = boostDraft.trim();
		const parsed = trimmed ? Number.parseInt(trimmed, 10) : 0;
		const next = Number.isFinite(parsed) && parsed > 0 ? parsed : null;
		if (next === currentBoost()) {
			// Normalise the buffer even when nothing moved, so a stray
			// "abc" or " 50 " does not linger as if it were persisted.
			boostDraft = next ? String(next) : '';
			return;
		}
		savingBoost = true;
		facetError = null;
		try {
			await write(deps.readFacets().name, next);
		} catch (error) {
			facetError = describe(error, 'Failed to set skill boost');
		}
		savingBoost = false;
		boostDraft = next ? String(next) : '';
	}

	/** Read the pickable quests fresh: playlists first (a playlist is the
	 * coarser declaration), then quests. Returns false when the read
	 * failed, so the caller can decline to open an empty menu. */
	async function loadQuestOptions(): Promise<boolean> {
		facetError = null;
		try {
			const [playlists, quests] = await Promise.all([deps.listPlaylists(), deps.listQuests()]);
			questOptions = [
				...playlists.map((playlist) => ({
					id: Number(playlist.id),
					name: playlist.name,
					isPlaylist: true,
				})),
				...quests.map((quest) => ({ id: Number(quest.id), name: quest.name, isPlaylist: false })),
			];
			return true;
		} catch (error) {
			facetError = describe(error, 'Failed to read quests');
			return false;
		}
	}

	async function declareQuest(id: number, isPlaylist: boolean) {
		questSaving = true;
		facetError = null;
		try {
			await deps.declareQuest(isPlaylist ? null : id, isPlaylist ? id : null);
			await deps.refresh();
		} catch (error) {
			facetError = describe(error, 'Failed to declare quest');
		}
		questSaving = false;
	}

	async function clearQuest() {
		questSaving = true;
		facetError = null;
		try {
			await deps.declareQuest(null, null);
			await deps.refresh();
		} catch (error) {
			facetError = describe(error, 'Failed to clear quest');
		}
		questSaving = false;
	}

	return {
		get nameQuery() {
			return nameQuery;
		},
		set nameQuery(value: string) {
			nameQuery = value;
		},
		get nameInput() {
			return nameInput;
		},
		set nameInput(value: HTMLInputElement | null) {
			nameInput = value;
		},
		get nameInputFocused() {
			return nameInputFocused;
		},
		get nameSuggestions() {
			return nameTypeahead.results;
		},
		get nameLoading() {
			return nameTypeahead.loading;
		},
		get nameError() {
			return nameError;
		},
		get savingName() {
			return savingName;
		},
		get boostDraft() {
			return boostDraft;
		},
		set boostDraft(value: string) {
			boostDraft = value;
		},
		get savingBoost() {
			return savingBoost;
		},
		/** Whether the name may still be set. The name is session-grain, so
		 * a live edit could only rewrite the whole session's history: it is
		 * fixed once a session runs, and correcting it is a post-hoc move on
		 * the session record. The boost has no such flag because its grain
		 * is finer than the session, so it always edits. */
		get nameEditable() {
			return !deps.isSessionActive();
		},
		get questSaving() {
			return questSaving;
		},
		get questOptions() {
			return questOptions;
		},
		get facetError() {
			return facetError;
		},
		set facetError(value: string | null) {
			facetError = value;
		},

		/** Drive the lookup from the input state: an emptied query (or a
		 * hidden input) suspends the search and dismisses the menu while
		 * keeping the typed text. */
		syncNameQuery(visible: boolean) {
			if (!visible) {
				nameTypeahead.cancel();
				void deps.closeNameMenu();
				return;
			}
			nameTypeahead.query = nameQuery;
			if (!nameQuery.trim()) {
				nameTypeahead.cancel();
				void deps.closeNameMenu();
				return;
			}
			nameTypeahead.refresh();
		},

		/** Mirror the settled lookup error and re-present the menu at each
		 * lifecycle transition while the input is focused or it is open. */
		presentNameMenu(visible: boolean, menuOpen: boolean) {
			nameError = nameTypeahead.error;
			if (!visible || !nameQuery.trim()) return;
			if (nameInputFocused || menuOpen) void deps.openNameMenu();
		},

		/** Keep the boost buffer in step with the persisted value while the
		 * user is not editing it. */
		syncBoostDraft() {
			if (savingBoost) return;
			const persisted = deps.readFacets().boost;
			boostDraft = persisted && persisted > 0 ? String(persisted) : '';
		},

		handleNameFocus() {
			clearNameCloseTimer();
			nameInputFocused = true;
			if (
				nameQuery.trim() &&
				(nameTypeahead.results.length > 0 || nameTypeahead.loading || !!nameError)
			) {
				void deps.openNameMenu();
			}
		},

		handleNameBlur() {
			nameInputFocused = false;
			clearNameCloseTimer();
			nameCloseTimer = setTimeout(() => {
				void deps.closeNameMenu();
			}, 120);
		},

		async handleNameKeydown(event: KeyboardEvent) {
			if (event.key === 'Escape') {
				await deps.closeNameMenu();
				return;
			}
			if (event.key !== 'Enter') return;
			event.preventDefault();
			await applyName(nameQuery.trim());
		},

		clearNameCloseTimer,
		applyName,
		clearName,
		commitBoost,
		loadQuestOptions,
		declareQuest,
		clearQuest,
		destroy() {
			clearNameCloseTimer();
			nameTypeahead.destroy();
		},
	};
}
