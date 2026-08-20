/** Runes-native state for protection setup, selection, and observations. */

import {
	archiveProtectionLoadout,
	archiveProtectionSet,
	confirmProtectionObservation,
	createProtectionLoadout,
	createProtectionSet,
	getProtectionOverview,
	type ProtectionEconomyKind,
	type ProtectionObservationOutcome,
	type ProtectionOverview,
	type ProtectionScanResult,
	type ProtectionSet,
	type ProtectionSetKind,
	scanTradeTerminalValue,
	selectProtectionLoadout,
	updateProtectionLoadout,
	updateProtectionSet,
} from '$lib/api';
import { describeError } from '$lib/view/errorState';

const EMPTY: ProtectionOverview = {
	sets: [],
	loadouts: [],
	activeLoadoutId: null,
	recentReconciliations: [],
	recentCostWindows: [],
};

function clientToken(): string {
	return (
		globalThis.crypto?.randomUUID?.() ??
		`protection-${Date.now()}-${Math.random().toString(36).slice(2)}`
	);
}

export function createProtectionModel() {
	let overview = $state<ProtectionOverview>({ ...EMPTY });
	let loading = $state(true);
	let saving = $state(false);
	let error = $state<string | null>(null);

	let setModalOpen = $state(false);
	let editingSetId = $state<string | null>(null);
	let setKind = $state<ProtectionSetKind>('armour');
	let setName = $state('');
	let setEconomyKind = $state<ProtectionEconomyKind>('limited');
	let setMarkup = $state('100');

	let loadoutModalOpen = $state(false);
	let editingLoadoutId = $state<string | null>(null);
	let loadoutName = $state('');
	let loadoutArmourId = $state('');
	let loadoutPlateId = $state('');

	let observationSet = $state<ProtectionSet | null>(null);
	let observationToken: string | null = null;
	let lastOutcome = $state<ProtectionObservationOutcome | null>(null);
	let removalTarget = $state<
		{ kind: 'set'; id: string; name: string } | { kind: 'loadout'; id: string; name: string } | null
	>(null);

	const armourSets = $derived(overview.sets.filter((set) => set.kind === 'armour'));
	const plateSets = $derived(overview.sets.filter((set) => set.kind === 'plates'));
	const activeLoadout = $derived(
		overview.loadouts.find((loadout) => loadout.id === overview.activeLoadoutId) ?? null,
	);

	function adopt(next: ProtectionOverview): void {
		overview = next;
	}

	async function load(guideMode = false): Promise<void> {
		loading = true;
		error = null;
		try {
			overview = guideMode ? { ...EMPTY } : await getProtectionOverview();
		} catch (cause) {
			error = describeError(cause, 'Failed to load protection');
		} finally {
			loading = false;
		}
	}

	function openSet(kind: ProtectionSetKind): void {
		editingSetId = null;
		setKind = kind;
		setName = '';
		setEconomyKind = 'limited';
		setMarkup = '100';
		setModalOpen = true;
	}

	function editSet(set: ProtectionSet): void {
		editingSetId = set.id;
		setKind = set.kind;
		setName = set.name;
		setEconomyKind = set.economyKind;
		setMarkup = String(set.markupPercent ?? 100);
		setModalOpen = true;
	}

	async function saveSet(): Promise<void> {
		const markup = Number(setMarkup);
		if (!setName.trim()) return;
		if (setEconomyKind === 'limited' && (!Number.isFinite(markup) || markup < 100)) return;
		saving = true;
		error = null;
		try {
			const input = {
				kind: setKind,
				name: setName.trim(),
				economyKind: setEconomyKind,
				markupPercent: setEconomyKind === 'limited' ? markup : null,
			};
			adopt(
				editingSetId
					? await updateProtectionSet(editingSetId, input)
					: await createProtectionSet(input),
			);
			setModalOpen = false;
		} catch (cause) {
			error = describeError(cause, 'Failed to create protection set');
		} finally {
			saving = false;
		}
	}

	function openLoadout(): void {
		editingLoadoutId = null;
		loadoutName = '';
		loadoutArmourId = '';
		loadoutPlateId = '';
		loadoutModalOpen = true;
	}

	function editLoadout(id: string): void {
		const loadout = overview.loadouts.find((candidate) => candidate.id === id);
		if (!loadout) return;
		editingLoadoutId = loadout.id;
		loadoutName = loadout.name;
		loadoutArmourId = loadout.armour?.id ?? '';
		loadoutPlateId = loadout.plates?.id ?? '';
		loadoutModalOpen = true;
	}

	function makeNoProtection(): void {
		editingLoadoutId = null;
		loadoutName = 'No protection';
		loadoutArmourId = '';
		loadoutPlateId = '';
		loadoutModalOpen = true;
	}

	async function saveLoadout(): Promise<void> {
		if (!loadoutName.trim()) return;
		saving = true;
		error = null;
		try {
			const input = {
				name: loadoutName.trim(),
				armourSetId: loadoutArmourId ? Number(loadoutArmourId) : null,
				plateSetId: loadoutPlateId ? Number(loadoutPlateId) : null,
			};
			let next = editingLoadoutId
				? await updateProtectionLoadout(editingLoadoutId, input)
				: await createProtectionLoadout(input);
			if (!editingLoadoutId && next.loadouts.length === 1 && next.activeLoadoutId === null) {
				next = await selectProtectionLoadout(next.loadouts[0].id);
			}
			adopt(next);
			loadoutModalOpen = false;
		} catch (cause) {
			error = describeError(cause, 'Failed to create protection loadout');
		} finally {
			saving = false;
		}
	}

	async function selectLoadout(id: string): Promise<void> {
		if (overview.activeLoadoutId === id || saving) return;
		saving = true;
		error = null;
		try {
			adopt(await selectProtectionLoadout(id));
		} catch (cause) {
			error = describeError(cause, 'Failed to select protection loadout');
		} finally {
			saving = false;
		}
	}

	async function archiveSet(id: string): Promise<void> {
		saving = true;
		error = null;
		try {
			adopt(await archiveProtectionSet(id));
			removalTarget = null;
		} catch (cause) {
			error = describeError(cause, 'Failed to archive protection set');
		} finally {
			saving = false;
		}
	}

	async function archiveLoadout(id: string): Promise<void> {
		saving = true;
		error = null;
		try {
			adopt(await archiveProtectionLoadout(id));
			removalTarget = null;
		} catch (cause) {
			error = describeError(cause, 'Failed to archive protection loadout');
		} finally {
			saving = false;
		}
	}

	function askRemoveSet(set: ProtectionSet): void {
		removalTarget = { kind: 'set', id: set.id, name: set.name };
	}

	function askRemoveLoadout(id: string): void {
		const loadout = overview.loadouts.find((candidate) => candidate.id === id);
		if (loadout) removalTarget = { kind: 'loadout', id: loadout.id, name: loadout.name };
	}

	async function confirmRemoval(): Promise<void> {
		if (!removalTarget) return;
		if (removalTarget.kind === 'set') await archiveSet(removalTarget.id);
		else await archiveLoadout(removalTarget.id);
	}

	function openObservation(set: ProtectionSet): void {
		observationSet = set;
		observationToken = clientToken();
		lastOutcome = null;
	}

	function closeObservation(): void {
		observationSet = null;
		observationToken = null;
	}

	async function scan(): Promise<ProtectionScanResult> {
		return scanTradeTerminalValue();
	}

	async function confirmObservation(input: {
		valuePed: number;
		source: 'ocr' | 'manual';
		rawText?: string | null;
		resetReason?: string | null;
	}): Promise<ProtectionObservationOutcome | null> {
		if (!observationSet) return null;
		saving = true;
		error = null;
		try {
			if (!observationToken) observationToken = clientToken();
			const outcome = await confirmProtectionObservation({
				setId: Number(observationSet.id),
				clientToken: observationToken,
				ttValuePed: input.valuePed,
				source: input.source,
				rawText: input.rawText ?? null,
				resetReason: input.resetReason ?? null,
			});
			lastOutcome = outcome;
			observationToken = null;
			try {
				overview = await getProtectionOverview();
				observationSet = overview.sets.find((set) => set.id === observationSet?.id) ?? null;
			} catch (cause) {
				error = `TT value recorded, but protection failed to refresh: ${describeError(cause, 'Unknown refresh error')}`;
			}
			return outcome;
		} catch (cause) {
			error = describeError(cause, 'Failed to record TT value');
			return null;
		} finally {
			saving = false;
		}
	}

	return {
		get overview() {
			return overview;
		},
		get loading() {
			return loading;
		},
		get saving() {
			return saving;
		},
		get error() {
			return error;
		},
		set error(value: string | null) {
			error = value;
		},
		get armourSets() {
			return armourSets;
		},
		get plateSets() {
			return plateSets;
		},
		get activeLoadout() {
			return activeLoadout;
		},
		get setModalOpen() {
			return setModalOpen;
		},
		get editingSetId() {
			return editingSetId;
		},
		set setModalOpen(value: boolean) {
			setModalOpen = value;
		},
		get setKind() {
			return setKind;
		},
		get setName() {
			return setName;
		},
		set setName(value: string) {
			setName = value;
		},
		get setEconomyKind() {
			return setEconomyKind;
		},
		set setEconomyKind(value: ProtectionEconomyKind) {
			setEconomyKind = value;
		},
		get setMarkup() {
			return setMarkup;
		},
		set setMarkup(value: string) {
			setMarkup = value;
		},
		get loadoutModalOpen() {
			return loadoutModalOpen;
		},
		get editingLoadoutId() {
			return editingLoadoutId;
		},
		set loadoutModalOpen(value: boolean) {
			loadoutModalOpen = value;
		},
		get loadoutName() {
			return loadoutName;
		},
		set loadoutName(value: string) {
			loadoutName = value;
		},
		get loadoutArmourId() {
			return loadoutArmourId;
		},
		set loadoutArmourId(value: string) {
			loadoutArmourId = value;
		},
		get loadoutPlateId() {
			return loadoutPlateId;
		},
		set loadoutPlateId(value: string) {
			loadoutPlateId = value;
		},
		get observationSet() {
			return observationSet;
		},
		get observationModalOpen() {
			return observationSet !== null;
		},
		set observationModalOpen(value: boolean) {
			if (!value) closeObservation();
		},
		get lastOutcome() {
			return lastOutcome;
		},
		get removalTarget() {
			return removalTarget;
		},
		get removalModalOpen() {
			return removalTarget !== null;
		},
		set removalModalOpen(value: boolean) {
			if (!value) removalTarget = null;
		},
		load,
		openSet,
		editSet,
		saveSet,
		openLoadout,
		editLoadout,
		makeNoProtection,
		saveLoadout,
		selectLoadout,
		archiveSet,
		archiveLoadout,
		askRemoveSet,
		askRemoveLoadout,
		confirmRemoval,
		openObservation,
		closeObservation,
		scan,
		confirmObservation,
	};
}

export type ProtectionModel = ReturnType<typeof createProtectionModel>;
