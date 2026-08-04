/**
 * Session-definitions view model: the definition list, the selection
 * write (which definition the next session starts as an instance of),
 * and the full-screen authoring environment's state (form, roster
 * draft, the morph lifecycle). Deps-injected so the dashboard island
 * and the tests compose it the same way.
 *
 * A definition's roster is authored data replaced wholesale on save
 * (the playlist-items precedent), so the draft here is a plain array
 * the editor mutates freely; nothing persists until Save.
 */

import type { SessionDefinition, SessionDefinitionInput, SessionRosterEntryKind } from '$lib/api';
import { ApiError } from '$lib/api';
import type { Quest, QuestFamily } from '$lib/types';
import { describeError } from '$lib/view/errorState';

/** One roster row as the editor drafts it (ids stringified for the UI;
 * `missing` marks a stored reference whose target has been deleted). */
export interface RosterDraftEntry {
	kind: SessionRosterEntryKind;
	refId: string | null;
	label: string | null;
	displayName: string;
	missing: boolean;
}

export interface DefinitionsModelDeps {
	listDefinitions(): Promise<SessionDefinition[]>;
	createDefinition(data: SessionDefinitionInput): Promise<SessionDefinition>;
	updateDefinition(id: string, data: SessionDefinitionInput): Promise<SessionDefinition>;
	deleteDefinition(id: string): Promise<void>;
	/** The tracking-family selection verb (null withdraws). */
	selectDefinition(id: number | null): Promise<unknown>;
	/** Re-read the tracking snapshot after a selection write. */
	refreshTracking(): Promise<unknown>;
	/** The roster's offer sources (loaded when the authoring opens). */
	listFamilies(): Promise<QuestFamily[]>;
	listQuests(): Promise<Quest[]>;
}

export type AuthoringMode = 'closed' | 'create' | 'edit';

export function createDefinitionsModel(deps: DefinitionsModelDeps) {
	let definitions = $state<SessionDefinition[]>([]);
	let loading = $state(false);
	let error = $state<string | null>(null);
	let selecting = $state(false);

	// ── The authoring environment ──
	let mode = $state<AuthoringMode>('closed');
	let editingId = $state<string | null>(null);
	let name = $state('');
	let adHocSegments = $state(false);
	let roster = $state<RosterDraftEntry[]>([]);
	let saving = $state(false);
	let authoringError = $state<string | null>(null);
	let deleteArmed = $state(false);

	// The roster's offer sources, loaded when the authoring opens so the
	// editor never presents a stale catalogue.
	let families = $state<QuestFamily[]>([]);
	let quests = $state<Quest[]>([]);
	let sourcesLoading = $state(false);

	async function loadDefinitions() {
		loading = true;
		try {
			definitions = await deps.listDefinitions();
			error = null;
		} catch (e) {
			error = describeError(e, 'Failed to load session types');
		} finally {
			loading = false;
		}
	}

	/** Select the definition the next session starts under (null: none).
	 * The backend writes the name facet with it in the same motion. */
	async function select(id: string | null) {
		selecting = true;
		try {
			await deps.selectDefinition(id === null ? null : Number(id));
			error = null;
			await deps.refreshTracking();
		} catch (e) {
			error = describeError(e, 'Failed to select session type');
		} finally {
			selecting = false;
		}
	}

	async function loadSources() {
		sourcesLoading = true;
		try {
			[families, quests] = await Promise.all([deps.listFamilies(), deps.listQuests()]);
		} catch (e) {
			authoringError = describeError(e, 'Failed to load roster options');
		} finally {
			sourcesLoading = false;
		}
	}

	function openCreate() {
		mode = 'create';
		editingId = null;
		name = '';
		adHocSegments = false;
		roster = [];
		authoringError = null;
		deleteArmed = false;
		void loadSources();
	}

	function openEdit(definition: SessionDefinition) {
		mode = 'edit';
		editingId = definition.id;
		name = definition.name;
		adHocSegments = definition.adHocSegments;
		roster = definition.roster.map((entry) => ({
			kind: entry.kind,
			refId: entry.refId,
			label: entry.label,
			displayName: entry.displayName ?? entry.label ?? '(removed)',
			missing: entry.displayName === null,
		}));
		authoringError = null;
		deleteArmed = false;
		void loadSources();
	}

	function close() {
		mode = 'closed';
		editingId = null;
		deleteArmed = false;
	}

	// ── Roster drafting ──

	function hasReference(kind: SessionRosterEntryKind, refId: string) {
		return roster.some((entry) => entry.kind === kind && entry.refId === refId);
	}

	function addFamily(family: QuestFamily) {
		if (hasReference('quest_family', family.id)) return;
		roster = [
			...roster,
			{
				kind: 'quest_family',
				refId: family.id,
				label: null,
				displayName: family.name,
				missing: false,
			},
		];
	}

	function addQuest(quest: Quest) {
		if (hasReference('quest', quest.id)) return;
		roster = [
			...roster,
			{ kind: 'quest', refId: quest.id, label: null, displayName: quest.name, missing: false },
		];
	}

	function addSegment(label: string) {
		const trimmed = label.trim();
		if (!trimmed) return;
		roster = [
			...roster,
			{ kind: 'segment', refId: null, label: trimmed, displayName: trimmed, missing: false },
		];
	}

	function removeEntry(index: number) {
		roster = roster.filter((_, i) => i !== index);
	}

	function moveEntry(index: number, delta: -1 | 1) {
		const target = index + delta;
		if (target < 0 || target >= roster.length) return;
		const next = [...roster];
		const [entry] = next.splice(index, 1);
		next.splice(target, 0, entry);
		roster = next;
	}

	// ── Persistence ──

	function toInput(): SessionDefinitionInput {
		return {
			name: name.trim(),
			ad_hoc_segments: adHocSegments,
			// A dead reference is dropped on save: keeping it would fail
			// the server's active-target validation, and the editor showed
			// the hole explicitly before this point.
			roster: roster
				.filter((entry) => !entry.missing)
				.map((entry) => ({
					kind: entry.kind,
					ref_id: entry.refId === null ? null : Number(entry.refId),
					label: entry.label,
				})),
		};
	}

	/** Persist the draft. On a create, the new definition becomes the
	 * selection in the same motion (the environment contracts back with
	 * it selected); with a session running the selection is fixed, so a
	 * 409 there leaves the save intact and quiet. */
	async function save(): Promise<boolean> {
		if (!name.trim()) {
			authoringError = 'A session type needs a name';
			return false;
		}
		saving = true;
		authoringError = null;
		try {
			if (editingId !== null) {
				await deps.updateDefinition(editingId, toInput());
			} else {
				const created = await deps.createDefinition(toInput());
				try {
					await deps.selectDefinition(Number(created.id));
				} catch (e) {
					if (!(e instanceof ApiError && e.kind === 'conflict')) throw e;
				}
			}
			await loadDefinitions();
			await deps.refreshTracking();
			close();
			return true;
		} catch (e) {
			authoringError = describeError(e, 'Failed to save session type');
			return false;
		} finally {
			saving = false;
		}
	}

	/** Delete the definition being edited (two-step: arm, then confirm).
	 * Instances keep their stamped reference; the type just stops being
	 * offered. */
	async function deleteEditing(): Promise<boolean> {
		if (editingId === null) return false;
		if (!deleteArmed) {
			deleteArmed = true;
			return false;
		}
		saving = true;
		authoringError = null;
		try {
			await deps.deleteDefinition(editingId);
			await loadDefinitions();
			await deps.refreshTracking();
			close();
			return true;
		} catch (e) {
			authoringError = describeError(e, 'Failed to delete session type');
			return false;
		} finally {
			saving = false;
		}
	}

	return {
		get definitions() {
			return definitions;
		},
		get loading() {
			return loading;
		},
		get error() {
			return error;
		},
		set error(value: string | null) {
			error = value;
		},
		get selecting() {
			return selecting;
		},
		get mode() {
			return mode;
		},
		get editingId() {
			return editingId;
		},
		get name() {
			return name;
		},
		set name(value: string) {
			name = value;
		},
		get adHocSegments() {
			return adHocSegments;
		},
		set adHocSegments(value: boolean) {
			adHocSegments = value;
		},
		get roster() {
			return roster;
		},
		get saving() {
			return saving;
		},
		get authoringError() {
			return authoringError;
		},
		set authoringError(value: string | null) {
			authoringError = value;
		},
		get deleteArmed() {
			return deleteArmed;
		},
		set deleteArmed(value: boolean) {
			deleteArmed = value;
		},
		get families() {
			return families;
		},
		get quests() {
			return quests;
		},
		get sourcesLoading() {
			return sourcesLoading;
		},

		loadDefinitions,
		select,
		openCreate,
		openEdit,
		close,
		addFamily,
		addQuest,
		addSegment,
		removeEntry,
		moveEntry,
		save,
		deleteEditing,
	};
}

export type DefinitionsModel = ReturnType<typeof createDefinitionsModel>;
