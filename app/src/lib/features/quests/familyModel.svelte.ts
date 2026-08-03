/**
 * Quest-family view model: the family form modal (create/edit) and the
 * family CRUD handlers. Depends on the quests model for the family list,
 * the quest catalogue (member counts and post-change refresh), the shared
 * error strip, and the shared menu delete-confirm state.
 *
 * A family is the availability model for variants of one repeatable slot:
 * it carries the cooldown (hours plus an anchor) and its members cool as a
 * unit. Creating or renaming one sweeps matching "Family: Variant" quests
 * in server-side, so the quest list is re-read after every family write.
 */

import { createQuestFamily, deleteQuestFamily, updateQuestFamily } from '$lib/api';
import type { QuestCooldownAnchor, QuestFamily, QuestFamilyCreateData } from '$lib/types';
import { describeError } from '$lib/view/errorState';
import type { CooldownUnit } from './questsModel.svelte';

export interface FamilyFormState {
	name: string;
	planet: string;
	cooldown_anchor: QuestCooldownAnchor;
}

function defaultFamilyForm(): FamilyFormState {
	return {
		name: '',
		planet: 'Calypso',
		// Collection-timed daily slots are the motivating case, so the
		// pickup anchor is the authoring default (the game starts the
		// giver's timer when the mission is handed over, not when it is
		// completed).
		cooldown_anchor: 'pickup',
	};
}

/** The slice of the quests model this model reads and writes. */
export interface FamilyModelDeps {
	families: QuestFamily[];
	error: string | null;
	deleteConfirmId: string | null;
	/** Re-read quests after a family write (membership sweeps happen server-side). */
	refreshQuests: () => Promise<void>;
}

export function createFamilyModel(deps: FamilyModelDeps) {
	// ── Family modal ──
	let showFamilyModal = $state(false);
	let editingFamily = $state<QuestFamily | null>(null);
	let familyForm = $state(defaultFamilyForm());
	let cooldownUnit = $state<CooldownUnit>('hours');
	let cooldownInput = $state<number | null>(null);

	// ── Family CRUD ──
	function openNewFamily() {
		editingFamily = null;
		familyForm = defaultFamilyForm();
		cooldownUnit = 'hours';
		cooldownInput = null;
		showFamilyModal = true;
	}

	function openEditFamily(family: QuestFamily) {
		editingFamily = family;
		const h = family.cooldownDurationHours;
		if (h != null && h >= 24 && h % 24 === 0) {
			cooldownUnit = 'days';
			cooldownInput = h / 24;
		} else {
			cooldownUnit = 'hours';
			cooldownInput = h;
		}
		familyForm = {
			name: family.name,
			planet: family.planet,
			cooldown_anchor: family.cooldownAnchor,
		};
		showFamilyModal = true;
	}

	async function saveFamily() {
		const cdHours =
			cooldownInput != null && cooldownInput > 0
				? cooldownUnit === 'days'
					? cooldownInput * 24
					: cooldownInput
				: null;
		const data: QuestFamilyCreateData = {
			name: familyForm.name,
			planet: familyForm.planet,
			cooldown_hours: cdHours,
			cooldown_anchor: familyForm.cooldown_anchor,
		};
		try {
			if (editingFamily) {
				const updated = await updateQuestFamily(editingFamily.id, data);
				deps.families = deps.families.map((f) => (f.id === updated.id ? updated : f));
			} else {
				const created = await createQuestFamily(data);
				deps.families = [...deps.families, created];
			}
			showFamilyModal = false;
			// The write may have swept variants in (create/rename), and a
			// cooldown change moves every member's availability picture.
			await deps.refreshQuests();
		} catch (e) {
			deps.error = describeError(e, 'Failed to save quest family');
		}
	}

	async function handleDeleteFamily(familyId: string) {
		try {
			await deleteQuestFamily(familyId);
			deps.families = deps.families.filter((f) => f.id !== familyId);
			deps.deleteConfirmId = null;
			// Members were detached server-side; their rows must stop
			// showing the family gate.
			await deps.refreshQuests();
		} catch (e) {
			deps.error = describeError(e, 'Failed to delete quest family');
		}
	}

	return {
		get showFamilyModal() {
			return showFamilyModal;
		},
		set showFamilyModal(value: boolean) {
			showFamilyModal = value;
		},
		get editingFamily() {
			return editingFamily;
		},
		set editingFamily(value: QuestFamily | null) {
			editingFamily = value;
		},
		get familyForm() {
			return familyForm;
		},
		get cooldownUnit() {
			return cooldownUnit;
		},
		set cooldownUnit(value: CooldownUnit) {
			cooldownUnit = value;
		},
		get cooldownInput() {
			return cooldownInput;
		},
		set cooldownInput(value: number | null) {
			cooldownInput = value;
		},

		openNewFamily,
		openEditFamily,
		saveFamily,
		handleDeleteFamily,
	};
}

export type FamilyModel = ReturnType<typeof createFamilyModel>;
