/**
 * Equipment-library view model: the library data set split by kind, the
 * row expansion and detail cache, the add/edit form with its catalogue
 * pickers, and the CRUD handlers. Presentation lives in the feature
 * components; they compose over this state.
 */

import {
	addToLibrary,
	type EquipmentSearchResult,
	getEquipmentDetail,
	getEquipmentLibrary,
	getSettings,
	hotbarFromSettings,
	removeFromLibrary,
	searchEquipmentItems,
	updateLibrary,
} from '$lib/api';
import {
	equipmentDemoDetails,
	equipmentDemoHotbar,
	equipmentDemoLibrary,
	equipmentDemoTrifecta,
} from '$lib/guide/fixtures/equipment';
import type { Equipment, EquipmentDetail, HealingTool } from '$lib/types';
import type { HarvestGuardrailSettings, Hotbar, TrifectaSettings } from '$lib/types/settings';
import { describeError } from '$lib/view/errorState';
import { createTypeahead } from '$lib/view/typeahead.svelte';
import { previewCostPerUse } from './costPreview';

export type EquipmentFormType = 'weapon' | 'healing' | 'consumable' | 'tool';

export function createLibraryModel() {
	// ── Data ──
	let allEquipment = $state<Equipment[]>([]);
	let equipmentList = $state<Equipment[]>([]);
	let healingTools = $state<HealingTool[]>([]);
	let consumables = $state<Equipment[]>([]);
	let harvestingTools = $state<Equipment[]>([]);
	let hotbar = $state<Hotbar>({});
	let hotbarHooksEnabled = $state(true);
	let trifecta = $state<TrifectaSettings>({
		activePresetId: null,
		activePresetName: null,
		presets: [],
		ready: false,
		message: null,
	});
	let harvestGuardrail = $state<HarvestGuardrailSettings>({
		enabled: false,
		shortToolId: null,
		longToolId: null,
		hugeToolId: null,
	});
	let error = $state<string | null>(null);

	// ── Rows ──
	let expandedId = $state<string | null>(null);
	let detailCache = $state<Record<string, EquipmentDetail>>({});

	// ── Form modal ──
	let showAddModal = $state(false);
	let addType = $state<EquipmentFormType>('weapon');
	let editingEquipmentId = $state<string | null>(null);
	let saving = $state(false);
	let showOptionalAttachments = $state(false);
	let markupPercent = $state(100);
	let scopeMarkupPercent = $state(100);
	let absorberMarkupPercent = $state(100);
	let damageEnhancers = $state(0);

	// ── Catalogue pickers ──
	const label = (item: EquipmentSearchResult) => item.name;
	const weaponPicker = createTypeahead<EquipmentSearchResult>({
		search: (q) => searchEquipmentItems(q, 'weapon'),
		labelOf: label,
	});
	// The amp list never offers the selected weapon back as its own amp.
	const ampPicker = createTypeahead<EquipmentSearchResult>({
		search: async (q) => {
			const results = await searchEquipmentItems(q, 'amp');
			return results.filter((r) => r.name !== weaponPicker.selected?.name);
		},
		labelOf: label,
	});
	const healerPicker = createTypeahead<EquipmentSearchResult>({
		search: (q) => searchEquipmentItems(q, 'healer'),
		labelOf: label,
	});
	const scopePicker = createTypeahead<EquipmentSearchResult>({
		search: (q) => searchEquipmentItems(q, 'scope'),
		labelOf: label,
	});
	const absorberPicker = createTypeahead<EquipmentSearchResult>({
		search: (q) => searchEquipmentItems(q, 'absorber'),
		labelOf: label,
	});
	const consumablePicker = createTypeahead<EquipmentSearchResult>({
		search: (q) => searchEquipmentItems(q, 'consumable'),
		labelOf: label,
	});
	const toolPicker = createTypeahead<EquipmentSearchResult>({
		search: (q) => searchEquipmentItems(q, 'tool'),
		labelOf: label,
	});
	const pickers = [
		weaponPicker,
		ampPicker,
		healerPicker,
		scopePicker,
		absorberPicker,
		consumablePicker,
		toolPicker,
	];

	// ── Computed ──
	const sortedEquipment = $derived([...equipmentList].sort((a, b) => a.name.localeCompare(b.name)));

	const liveCostPreview = $derived(
		previewCostPerUse({
			weapon: weaponPicker.selected,
			amp: ampPicker.selected,
			scope: scopePicker.selected,
			markupPercent,
			scopeMarkupPercent,
			damageEnhancers,
		}),
	);

	// Split the flat library into the per-kind lists the view renders. Weapons
	// stay unsorted here; the view's sorted derivation owns their ordering.
	function splitByKind(library: Equipment[]) {
		equipmentList = library.filter((e) => e.type === 'weapon');
		healingTools = library
			.filter((e) => e.type === 'healing')
			.map((e) => ({
				id: e.id,
				name: e.name,
				costPerHeal: e.costPerUse,
				isLimited: e.isLimited,
			}))
			.sort((a, b) => a.name.localeCompare(b.name));
		consumables = library
			.filter((e) => e.type === 'consumable')
			.sort((a, b) => a.name.localeCompare(b.name));
		harvestingTools = library
			.filter((e) => e.type === 'tool')
			.sort((a, b) => a.name.localeCompare(b.name));
	}

	async function loadData(guideMode: boolean): Promise<void> {
		try {
			if (guideMode) {
				const library = equipmentDemoLibrary.map((e) => ({ ...e }));
				allEquipment = library;
				splitByKind(library);
				hotbar = { ...equipmentDemoHotbar };
				hotbarHooksEnabled = true;
				trifecta = {
					...equipmentDemoTrifecta,
					presets: equipmentDemoTrifecta.presets.map((p) => ({ ...p })),
				};
				detailCache = Object.fromEntries(
					Object.entries(equipmentDemoDetails).map(([k, v]) => [k, { ...v }]),
				);
			} else {
				const [library, settings] = await Promise.all([getEquipmentLibrary(), getSettings()]);
				allEquipment = library;
				splitByKind(library);
				hotbar = hotbarFromSettings(settings);
				hotbarHooksEnabled = settings.hotbarHooksEnabled;
				trifecta = settings.trifecta;
				harvestGuardrail = settings.harvestGuardrail;
				detailCache = {};
			}
		} catch (e) {
			error = describeError(e, 'Failed to load equipment');
		}
	}

	function replaceEquipment(updated: Equipment) {
		allEquipment = allEquipment.some((item) => item.id === updated.id)
			? allEquipment.map((item) => (item.id === updated.id ? updated : item))
			: [...allEquipment, updated];
		splitByKind(allEquipment);
	}

	function openAddModal(prefill?: string, type: EquipmentFormType = 'weapon') {
		editingEquipmentId = null;
		addType = type;
		for (const picker of pickers) picker.clear();
		if (prefill) weaponPicker.query = prefill;
		showOptionalAttachments = false;
		markupPercent = 100;
		scopeMarkupPercent = 100;
		absorberMarkupPercent = 100;
		damageEnhancers = 0;
		showAddModal = true;
	}

	async function openEditModal(id: string) {
		let detail = detailCache[id];
		if (!detail) {
			try {
				detail = await getEquipmentDetail(id);
			} catch (e) {
				error = describeError(e, 'Failed to load equipment detail');
				return;
			}
		}
		detailCache[id] = detail;
		editingEquipmentId = id;
		addType = 'weapon';
		weaponPicker.select({
			catalogId: detail.weapon.catalogId,
			name: detail.weapon.name,
			decay: detail.weapon.decay,
			ammoBurn: detail.weapon.ammoBurn,
			markupPercent: detail.weapon.markupPercent,
			isLimited: detail.weapon.isLimited,
			damageEnhancers: detail.weapon.damageEnhancers,
		});
		if (detail.amplifier) {
			selectCompanion(ampPicker, detail.amplifier);
		} else {
			ampPicker.clear();
		}
		if (detail.scope) {
			scopePicker.select({
				catalogId: detail.scope.catalogId,
				name: detail.scope.name,
				decay: detail.scope.decay,
				ammoBurn: detail.scope.ammoBurn,
				markupPercent: detail.scope.markupPercent,
				isLimited: detail.scope.isLimited,
				damageEnhancers: detail.scope.damageEnhancers,
			});
		} else {
			scopePicker.clear();
		}
		if (detail.absorber) {
			selectCompanion(absorberPicker, detail.absorber);
		} else {
			absorberPicker.clear();
		}
		healerPicker.clear();
		showOptionalAttachments = !!detail.scope || !!detail.absorber;
		markupPercent = detail.weapon.markupPercent;
		scopeMarkupPercent = detail.scope?.markupPercent ?? 100;
		absorberMarkupPercent = detail.absorber?.markupPercent ?? 100;
		damageEnhancers = detail.weapon.damageEnhancers;
		showAddModal = true;
	}

	// Amp and absorber components seed with no enhancer slots of their own.
	function selectCompanion(
		picker: (typeof pickers)[number],
		component: {
			catalogId: string | null;
			name: string;
			decay: number;
			ammoBurn: number;
			markupPercent: number;
			isLimited: boolean;
		},
	) {
		picker.select({
			catalogId: component.catalogId,
			name: component.name,
			decay: component.decay,
			ammoBurn: component.ammoBurn,
			markupPercent: component.markupPercent,
			isLimited: component.isLimited,
			damageEnhancers: 0,
		});
	}

	function setAddType(type: EquipmentFormType) {
		addType = type;
		if (type === 'weapon') {
			healerPicker.clear();
			toolPicker.clear();
		} else if (type === 'healing') {
			weaponPicker.clear();
			ampPicker.clear();
			toolPicker.clear();
		} else if (type === 'tool') {
			weaponPicker.clear();
			ampPicker.clear();
			healerPicker.clear();
		} else {
			weaponPicker.clear();
			ampPicker.clear();
			healerPicker.clear();
			toolPicker.clear();
		}
	}

	function selectConsumableCustom(name: string) {
		const trimmed = name.trim();
		if (!trimmed) return;
		consumablePicker.select({
			catalogId: null,
			name: trimmed,
			decay: 0,
			ammoBurn: 0,
			isLimited: false,
		});
	}

	async function toggleExpand(id: string) {
		if (expandedId === id) {
			expandedId = null;
			return;
		}
		expandedId = id;
		if (!detailCache[id]) {
			try {
				detailCache[id] = await getEquipmentDetail(id);
			} catch (e) {
				error = describeError(e, 'Failed to load equipment detail');
			}
		}
	}

	async function saveEquipment() {
		error = null;
		saving = true;
		try {
			if (addType === 'weapon') {
				const weapon = weaponPicker.selected;
				if (!weapon?.catalogId) return;
				const payload = {
					type: 'weapon' as const,
					catalog_id: weapon.catalogId,
					amp_catalog_id: ampPicker.selected?.catalogId ?? null,
					scope_catalog_id: scopePicker.selected?.catalogId ?? null,
					absorber_catalog_id: absorberPicker.selected?.catalogId ?? null,
					weapon_markup: weapon.isLimited ? markupPercent : 100,
					amp_markup: ampPicker.selected?.isLimited ? markupPercent : 100,
					scope_markup: scopePicker.selected?.isLimited ? scopeMarkupPercent : 100,
					absorber_markup: absorberPicker.selected?.isLimited ? absorberMarkupPercent : 100,
					damage_enhancers: damageEnhancers,
				};
				const item = editingEquipmentId
					? await updateLibrary(editingEquipmentId, payload)
					: await addToLibrary(payload);
				replaceEquipment(item);
				// The save has succeeded, so close the form before the detail fetch:
				// a fetch failure must not leave the modal open with a retry path
				// that would create a duplicate entry.
				showAddModal = false;
				editingEquipmentId = null;
				detailCache[item.id] = await getEquipmentDetail(item.id);
			} else if (addType === 'healing') {
				const healer = healerPicker.selected;
				if (!healer?.catalogId) return;
				const item = await addToLibrary({
					type: 'healing',
					catalog_id: healer.catalogId,
					weapon_markup: healer.isLimited ? markupPercent : 100,
				});
				replaceEquipment(item);
			} else if (addType === 'tool') {
				const tool = toolPicker.selected;
				if (!tool?.catalogId) return;
				const item = await addToLibrary({
					type: 'tool',
					catalog_id: tool.catalogId,
					weapon_markup: tool.isLimited ? markupPercent : 100,
				});
				replaceEquipment(item);
			} else {
				const consumable = consumablePicker.selected;
				if (!consumable) return;
				const item = await addToLibrary({
					type: 'consumable',
					catalog_id: consumable.catalogId ?? null,
					name: consumable.catalogId ? null : consumable.name,
				});
				replaceEquipment(item);
			}
			showAddModal = false;
			editingEquipmentId = null;
		} catch (e) {
			error = describeError(e, 'Failed to save equipment');
		} finally {
			saving = false;
		}
	}

	async function removeEquipment(id: string, type: EquipmentFormType = 'weapon') {
		error = null;
		try {
			await removeFromLibrary(id);
			allEquipment = allEquipment.filter((e) => e.id !== id);
			if (type === 'healing') {
				healingTools = healingTools.filter((e) => e.id !== id);
			} else if (type === 'consumable') {
				consumables = consumables.filter((e) => e.id !== id);
			} else if (type === 'tool') {
				harvestingTools = harvestingTools.filter((e) => e.id !== id);
			} else {
				equipmentList = equipmentList.filter((e) => e.id !== id);
			}
			if (expandedId === id) expandedId = null;
			delete detailCache[id];
		} catch (e) {
			error = describeError(e, 'Failed to remove equipment');
		}
	}

	function destroy() {
		for (const picker of pickers) picker.destroy();
	}

	return {
		// ── Data ──
		get allEquipment() {
			return allEquipment;
		},
		get sortedEquipment() {
			return sortedEquipment;
		},
		get healingTools() {
			return healingTools;
		},
		get consumables() {
			return consumables;
		},
		get harvestingTools() {
			return harvestingTools;
		},
		get hotbar() {
			return hotbar;
		},
		set hotbar(value: Hotbar) {
			hotbar = value;
		},
		get hotbarHooksEnabled() {
			return hotbarHooksEnabled;
		},
		get trifecta() {
			return trifecta;
		},
		set trifecta(value: TrifectaSettings) {
			trifecta = value;
		},
		get harvestGuardrail() {
			return harvestGuardrail;
		},
		set harvestGuardrail(value: HarvestGuardrailSettings) {
			harvestGuardrail = value;
		},
		get error() {
			return error;
		},
		set error(value: string | null) {
			error = value;
		},

		// ── Rows ──
		get expandedId() {
			return expandedId;
		},
		set expandedId(value: string | null) {
			expandedId = value;
		},
		get detailCache() {
			return detailCache;
		},

		// ── Form modal ──
		get showAddModal() {
			return showAddModal;
		},
		set showAddModal(value: boolean) {
			showAddModal = value;
			// Any close path (cancel, backdrop, escape) drops the edit target.
			if (!value) editingEquipmentId = null;
		},
		get addType() {
			return addType;
		},
		get editingEquipmentId() {
			return editingEquipmentId;
		},
		get saving() {
			return saving;
		},
		get showOptionalAttachments() {
			return showOptionalAttachments;
		},
		set showOptionalAttachments(value: boolean) {
			showOptionalAttachments = value;
		},
		get markupPercent() {
			return markupPercent;
		},
		set markupPercent(value: number) {
			markupPercent = value;
		},
		get scopeMarkupPercent() {
			return scopeMarkupPercent;
		},
		set scopeMarkupPercent(value: number) {
			scopeMarkupPercent = value;
		},
		get absorberMarkupPercent() {
			return absorberMarkupPercent;
		},
		set absorberMarkupPercent(value: number) {
			absorberMarkupPercent = value;
		},
		get damageEnhancers() {
			return damageEnhancers;
		},
		set damageEnhancers(value: number) {
			damageEnhancers = value;
		},
		get liveCostPreview() {
			return liveCostPreview;
		},

		// ── Pickers ──
		weaponPicker,
		ampPicker,
		healerPicker,
		scopePicker,
		absorberPicker,
		consumablePicker,
		toolPicker,

		loadData,
		openAddModal,
		openEditModal,
		setAddType,
		selectConsumableCustom,
		toggleExpand,
		saveEquipment,
		removeEquipment,
		destroy,
	};
}

export type LibraryModel = ReturnType<typeof createLibraryModel>;
