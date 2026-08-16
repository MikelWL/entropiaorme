/** Runes-native state for protection setup, selection, and observations. */

import {
	archiveProtectionLoadout,
	archiveProtectionSet,
	confirmProtectionObservation,
	createProtectionLoadout,
	createProtectionSet,
	getProtectionOverview,
	scanTradeTerminalValue,
	selectProtectionLoadout,
	type ProtectionEconomyKind,
	type ProtectionObservationOutcome,
	type ProtectionOverview,
	type ProtectionScanResult,
	type ProtectionSet,
	type ProtectionSetKind,
} from '$lib/api';
import { describeError } from '$lib/view/errorState';

const EMPTY: ProtectionOverview = {
	sets: [],
	loadouts: [],
	activeLoadoutId: null,
	recentReconciliations: [],
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
	let setKind = $state<ProtectionSetKind>('armour');
	let setName = $state('');
	let setEconomyKind = $state<ProtectionEconomyKind>('limited');
	let setMarkup = $state('100');

	let loadoutModalOpen = $state(false);
	let loadoutName = $state('');
	let loadoutArmourId = $state('');
	let loadoutPlateId = $state('');

	let observationSet = $state<ProtectionSet | null>(null);
	let lastOutcome = $state<ProtectionObservationOutcome | null>(null);

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
		setKind = kind;
		setName = '';
		setEconomyKind = 'limited';
		setMarkup = '100';
		setModalOpen = true;
	}

	async function saveSet(): Promise<void> {
		const markup = Number(setMarkup);
		if (!setName.trim()) return;
		if (setEconomyKind === 'limited' && (!Number.isFinite(markup) || markup < 100)) return;
		saving = true;
		error = null;
		try {
			adopt(
				await createProtectionSet({
					kind: setKind,
					name: setName.trim(),
					economyKind: setEconomyKind,
					markupPercent: setEconomyKind === 'limited' ? markup : null,
				}),
			);
			setModalOpen = false;
		} catch (cause) {
			error = describeError(cause, 'Failed to create protection set');
		} finally {
			saving = false;
		}
	}

	function openLoadout(): void {
		loadoutName = '';
		loadoutArmourId = '';
		loadoutPlateId = '';
		loadoutModalOpen = true;
	}

	function makeNoProtection(): void {
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
			let next = await createProtectionLoadout({
				name: loadoutName.trim(),
				armourSetId: loadoutArmourId ? Number(loadoutArmourId) : null,
				plateSetId: loadoutPlateId ? Number(loadoutPlateId) : null,
			});
			if (next.loadouts.length === 1 && next.activeLoadoutId === null) {
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
		} catch (cause) {
			error = describeError(cause, 'Failed to archive protection loadout');
		} finally {
			saving = false;
		}
	}

	function openObservation(set: ProtectionSet): void {
		observationSet = set;
		lastOutcome = null;
	}

	function closeObservation(): void {
		observationSet = null;
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
			const outcome = await confirmProtectionObservation({
				setId: Number(observationSet.id),
				clientToken: clientToken(),
				ttValuePed: input.valuePed,
				source: input.source,
				rawText: input.rawText ?? null,
				resetReason: input.resetReason ?? null,
			});
			lastOutcome = outcome;
			overview = await getProtectionOverview();
			observationSet = overview.sets.find((set) => set.id === observationSet?.id) ?? null;
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
			if (!value) observationSet = null;
		},
		get lastOutcome() {
			return lastOutcome;
		},
		load,
		openSet,
		saveSet,
		openLoadout,
		makeNoProtection,
		saveLoadout,
		selectLoadout,
		archiveSet,
		archiveLoadout,
		openObservation,
		closeObservation,
		scan,
		confirmObservation,
	};
}

export type ProtectionModel = ReturnType<typeof createProtectionModel>;
