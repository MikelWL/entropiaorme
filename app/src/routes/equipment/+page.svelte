<script lang="ts">
	import { onMount } from 'svelte';
	import { Badge, Button, Card, Divider, Input, Modal, SearchInput, SegmentedControl, Tabs } from '$lib/components';
	import type { Equipment, EquipmentDetail, HealingTool } from '$lib/types';
	import type { Hotbar, TrifectaSettings } from '$lib/types/settings';
	import {
		searchEquipmentItems,
		getEquipmentLibrary,
		addToLibrary,
		updateLibrary,
		removeFromLibrary,
		getEquipmentDetail,
		getSettings,
		hotbarFromSettings,
		type EquipmentSearchResult
	} from '$lib/api';
	import { getPreference } from '$lib/preferences';
	import { guideState, registerDemoApi, unregisterDemoApi } from '$lib/guide/state.svelte';
	import { closeGuide, openGuide } from '$lib/guide/engine';
	import { equipmentSurface } from '$lib/guide/surfaces/equipment';
	import {
		equipmentDemoLibrary,
		equipmentDemoDetails,
		equipmentDemoTrifecta,
		equipmentDemoHotbar
	} from '$lib/guide/fixtures/equipment';
	import TrifectaTab from './TrifectaTab.svelte';
	import HotbarTab from './HotbarTab.svelte';
	import { IconWeapons, IconConsumables, IconHealing } from '$lib/icons';

	const tabs = [
		{ id: 'library', label: 'Library' },
		{ id: 'trifecta', label: 'Trifecta' },
		{ id: 'hotbar', label: 'Hotbar' }
	];
	let activeTab = $state('library');

	// ── State ──
	let equipmentList: Equipment[] = $state([]);
	let allEquipment: Equipment[] = $state([]);
	let healingTools: HealingTool[] = $state([]);
	let hotbar: Hotbar = $state({});
	let hotbarHooksEnabled = $state(true);
	let trifecta: TrifectaSettings = $state({
		activePresetId: null,
		activePresetName: null,
		presets: [],
		ready: false,
		message: null
	});

	let expandedId: string | null = $state(null);
	let detailCache: Record<string, EquipmentDetail> = $state({});

	// Add equipment modal
	let showAddModal = $state(false);
	let addType: 'weapon' | 'healing' | 'consumable' = $state('weapon');
	let weaponSearch = $state('');
	let selectedWeapon: EquipmentSearchResult | null = $state(null);
	let weaponSearchResults: EquipmentSearchResult[] = $state([]);
	let ampSearch = $state('');
	let selectedAmp: EquipmentSearchResult | null = $state(null);
	let ampSearchResults: EquipmentSearchResult[] = $state([]);
	let healerSearch = $state('');
	let selectedHealer: EquipmentSearchResult | null = $state(null);
	let healerSearchResults: EquipmentSearchResult[] = $state([]);
	let scopeSearch = $state('');
	let selectedScope: EquipmentSearchResult | null = $state(null);
	let scopeSearchResults: EquipmentSearchResult[] = $state([]);
	let absorberSearch = $state('');
	let selectedAbsorber: EquipmentSearchResult | null = $state(null);
	let absorberSearchResults: EquipmentSearchResult[] = $state([]);
	let consumableSearch = $state('');
	let selectedConsumable: EquipmentSearchResult | null = $state(null);
	let consumableSearchResults: EquipmentSearchResult[] = $state([]);
	let consumables: Equipment[] = $state([]);
	let showOptionalAttachments = $state(false);
	let markupPercent = $state(100);
	let scopeMarkupPercent = $state(100);
	let absorberMarkupPercent = $state(100);
	let damageEnhancers = $state(0);
	let editingEquipmentId: string | null = $state(null);
	let saving = $state(false);
	let pageError: string | null = $state(null);

	// Guide-mode demo state for the hotbar/trifecta mutex (only consulted when guideState.isActive)
	let demoHotbarEnabled = $state(true);
	let guideSeen = $state(true);

	async function loadData(guideMode: boolean): Promise<void> {
		try {
			if (guideMode) {
				const library = equipmentDemoLibrary.map((e) => ({ ...e }));
				allEquipment = library;
				equipmentList = library.filter((e) => e.type === 'weapon');
				healingTools = library
					.filter((e) => e.type === 'healing')
					.map((e) => ({ id: e.id, name: e.name, costPerHeal: e.costPerUse, isLimited: e.isLimited }))
					.sort((a, b) => a.name.localeCompare(b.name));
				consumables = library
					.filter((e) => e.type === 'consumable')
					.sort((a, b) => a.name.localeCompare(b.name));
				hotbar = { ...equipmentDemoHotbar };
				hotbarHooksEnabled = true;
				trifecta = {
					...equipmentDemoTrifecta,
					presets: equipmentDemoTrifecta.presets.map((p) => ({ ...p }))
				};
				detailCache = Object.fromEntries(
					Object.entries(equipmentDemoDetails).map(([k, v]) => [k, { ...v }])
				);
			} else {
				const [library, settings] = await Promise.all([getEquipmentLibrary(), getSettings()]);
				allEquipment = library;
				equipmentList = library.filter((e) => e.type === 'weapon');
				healingTools = library
					.filter((e) => e.type === 'healing')
					.map((e) => ({ id: e.id, name: e.name, costPerHeal: e.costPerUse, isLimited: e.isLimited }))
					.sort((a, b) => a.name.localeCompare(b.name));
				consumables = library
					.filter((e) => e.type === 'consumable')
					.sort((a, b) => a.name.localeCompare(b.name));
				hotbar = hotbarFromSettings(settings);
				hotbarHooksEnabled = settings.hotbarHooksEnabled;
				trifecta = settings.trifecta;
				detailCache = {};
			}
		} catch (e) {
			pageError = e instanceof Error ? e.message : 'Failed to load equipment';
		}
	}

	// Reload data on initial mount and whenever guide-mode toggles.
	$effect(() => {
		void loadData(guideState.isActive);
	});

	onMount(() => {
		void (async () => {
			guideSeen = await getPreference<boolean>('guide_seen_equipment', false);
		})();
		registerDemoApi('equipment', {
			setActiveTab: (tab: string) => {
				activeTab = tab;
			},
			expandRow: (id: string) => {
				expandedId = id;
			},
			collapseRow: () => {
				expandedId = null;
			},
			openAddModal: (type: 'weapon' | 'healing' | 'consumable' = 'weapon') => {
				openAddModal(undefined, type);
			},
			closeAddModal: () => {
				showAddModal = false;
				editingEquipmentId = null;
			},
			setShowOptionalAttachments: (value: boolean) => {
				showOptionalAttachments = value;
			},
			setDemoHotbarEnabled: (value: boolean) => {
				demoHotbarEnabled = value;
			}
		});
		return () => unregisterDemoApi('equipment');
	});

	function toggleSurfaceGuide(): void {
		if (guideState.isActive) {
			closeGuide();
		} else {
			guideSeen = true;
			void openGuide(equipmentSurface);
		}
	}

	// ── Async search via $effect ──
	$effect(() => {
		const q = weaponSearch;
		if (selectedWeapon || q.length < 2) {
			weaponSearchResults = [];
			return;
		}
		const timeout = setTimeout(async () => {
			weaponSearchResults = await searchEquipmentItems(q, 'weapon');
		}, 200);
		return () => clearTimeout(timeout);
	});

	$effect(() => {
		const q = ampSearch;
		if (selectedAmp || q.length < 2) {
			ampSearchResults = [];
			return;
		}
		const timeout = setTimeout(async () => {
			const results = await searchEquipmentItems(q, 'amp');
			ampSearchResults = results.filter((r) => r.name !== selectedWeapon?.name);
		}, 200);
		return () => clearTimeout(timeout);
	});

	$effect(() => {
		const q = healerSearch;
		if (selectedHealer || q.length < 2) {
			healerSearchResults = [];
			return;
		}
		const timeout = setTimeout(async () => {
			healerSearchResults = await searchEquipmentItems(q, 'healer');
		}, 200);
		return () => clearTimeout(timeout);
	});

	$effect(() => {
		const q = scopeSearch;
		if (selectedScope || q.length < 2) {
			scopeSearchResults = [];
			return;
		}
		const timeout = setTimeout(async () => {
			scopeSearchResults = await searchEquipmentItems(q, 'scope');
		}, 200);
		return () => clearTimeout(timeout);
	});

	$effect(() => {
		const q = absorberSearch;
		if (selectedAbsorber || q.length < 2) {
			absorberSearchResults = [];
			return;
		}
		const timeout = setTimeout(async () => {
			absorberSearchResults = await searchEquipmentItems(q, 'absorber');
		}, 200);
		return () => clearTimeout(timeout);
	});

	$effect(() => {
		const q = consumableSearch;
		if (selectedConsumable || q.length < 2) {
			consumableSearchResults = [];
			return;
		}
		const timeout = setTimeout(async () => {
			consumableSearchResults = await searchEquipmentItems(q, 'consumable');
		}, 200);
		return () => clearTimeout(timeout);
	});

	$effect(() => {
		if (!showAddModal && editingEquipmentId) {
			editingEquipmentId = null;
		}
	});

	// ── Derived ──
	let sortedEquipment = $derived(
		[...equipmentList].sort((a, b) => a.name.localeCompare(b.name))
	);

	// Local cost preview (formula mirrors the backend cost engine's
	// per-use pricing for instant feedback)
	let liveCostPreview = $derived.by(() => {
		if (!selectedWeapon) return null;
		const weaponMult = selectedWeapon.isLimited ? markupPercent / 100 : 1.0;
		const enhancerMult = 1 + damageEnhancers * 0.1;
		let cost = selectedWeapon.decay * weaponMult * enhancerMult + selectedWeapon.ammoBurn * enhancerMult;
		if (selectedAmp) {
			const ampMult = selectedAmp.isLimited ? markupPercent / 100 : 1.0;
			cost += selectedAmp.decay * ampMult + selectedAmp.ammoBurn;
		}
		if (selectedScope) {
			const scopeMult = selectedScope.isLimited ? scopeMarkupPercent / 100 : 1.0;
			cost += selectedScope.decay * scopeMult;
		}
		return cost;
	});

	// ── Helpers ──
	// The wire carries the enrichment level as a plain number (0-3 by
	// construction); anything outside the ladder reads as unresolved.
	function enrichmentLabel(level: number): string {
		const labels = ['Unresolved', 'Base', 'Base + Amp', 'Full Setup'];
		return labels[level] ?? labels[0];
	}

	function enrichmentColor(level: number): 'negative' | 'warning' | 'accent' | 'positive' {
		const colors: ('negative' | 'warning' | 'accent' | 'positive')[] = [
			'negative',
			'warning',
			'accent',
			'positive'
		];
		return colors[level] ?? colors[0];
	}

	function formatPec(pec: number): string {
		return pec.toFixed(2);
	}

	function openAddModal(prefill?: string, type: 'weapon' | 'healing' | 'consumable' = 'weapon') {
		editingEquipmentId = null;
		addType = type;
		weaponSearch = prefill ?? '';
		selectedWeapon = null;
		weaponSearchResults = [];
		ampSearch = '';
		selectedAmp = null;
		ampSearchResults = [];
		healerSearch = '';
		selectedHealer = null;
		healerSearchResults = [];
		scopeSearch = '';
		selectedScope = null;
		scopeSearchResults = [];
		absorberSearch = '';
		selectedAbsorber = null;
		absorberSearchResults = [];
		consumableSearch = '';
		selectedConsumable = null;
		consumableSearchResults = [];
		showOptionalAttachments = false;
		markupPercent = 100;
		scopeMarkupPercent = 100;
		absorberMarkupPercent = 100;
		damageEnhancers = 0;
		showAddModal = true;
	}

	function replaceEquipment(updated: Equipment) {
		allEquipment = allEquipment.some((item) => item.id === updated.id)
			? allEquipment.map((item) => (item.id === updated.id ? updated : item))
			: [...allEquipment, updated];
		equipmentList = allEquipment
			.filter((item) => item.type === 'weapon')
			.sort((a, b) => a.name.localeCompare(b.name));
		healingTools = allEquipment
			.filter((item) => item.type === 'healing')
			.map((item) => ({ id: item.id, name: item.name, costPerHeal: item.costPerUse, isLimited: item.isLimited }))
			.sort((a, b) => a.name.localeCompare(b.name));
		consumables = allEquipment
			.filter((item) => item.type === 'consumable')
			.sort((a, b) => a.name.localeCompare(b.name));
	}

	async function openEditModal(id: string) {
		const detail = detailCache[id] ?? await getEquipmentDetail(id);
		detailCache[id] = detail;
		editingEquipmentId = id;
		addType = 'weapon';
		selectedWeapon = {
			catalogId: detail.weapon.catalogId,
			name: detail.weapon.name,
			decay: detail.weapon.decay,
			ammoBurn: detail.weapon.ammoBurn,
			markupPercent: detail.weapon.markupPercent,
			isLimited: detail.weapon.isLimited,
			damageEnhancers: detail.weapon.damageEnhancers,
		};
		weaponSearch = detail.weapon.name;
		weaponSearchResults = [];
		selectedAmp = detail.amplifier ? {
			catalogId: detail.amplifier.catalogId,
			name: detail.amplifier.name,
			decay: detail.amplifier.decay,
			ammoBurn: detail.amplifier.ammoBurn,
			markupPercent: detail.amplifier.markupPercent,
			isLimited: detail.amplifier.isLimited,
			damageEnhancers: 0,
		} : null;
		ampSearch = detail.amplifier?.name ?? '';
		ampSearchResults = [];
		selectedScope = detail.scope ? {
			catalogId: detail.scope.catalogId,
			name: detail.scope.name,
			decay: detail.scope.decay,
			ammoBurn: detail.scope.ammoBurn,
			markupPercent: detail.scope.markupPercent,
			isLimited: detail.scope.isLimited,
			damageEnhancers: detail.scope.damageEnhancers,
		} : null;
		scopeSearch = detail.scope?.name ?? '';
		scopeSearchResults = [];
		selectedAbsorber = detail.absorber ? {
			catalogId: detail.absorber.catalogId,
			name: detail.absorber.name,
			decay: detail.absorber.decay,
			ammoBurn: detail.absorber.ammoBurn,
			markupPercent: detail.absorber.markupPercent,
			isLimited: detail.absorber.isLimited,
			damageEnhancers: 0,
		} : null;
		absorberSearch = detail.absorber?.name ?? '';
		absorberSearchResults = [];
		selectedHealer = null;
		healerSearch = '';
		healerSearchResults = [];
		showOptionalAttachments = !!detail.scope || !!detail.absorber;
		markupPercent = detail.weapon.markupPercent;
		scopeMarkupPercent = detail.scope?.markupPercent ?? 100;
		absorberMarkupPercent = detail.absorber?.markupPercent ?? 100;
		damageEnhancers = detail.weapon.damageEnhancers;
		showAddModal = true;
	}

	function selectWeapon(w: EquipmentSearchResult) {
		selectedWeapon = w;
		weaponSearch = w.name;
		weaponSearchResults = [];
	}

	function selectAmp(a: EquipmentSearchResult) {
		selectedAmp = a;
		ampSearch = a.name;
		ampSearchResults = [];
	}

	function selectHealer(h: EquipmentSearchResult) {
		selectedHealer = h;
		healerSearch = h.name;
		healerSearchResults = [];
	}

	function selectScope(s: EquipmentSearchResult) {
		selectedScope = s;
		scopeSearch = s.name;
		scopeSearchResults = [];
	}

	function selectAbsorber(a: EquipmentSearchResult) {
		selectedAbsorber = a;
		absorberSearch = a.name;
		absorberSearchResults = [];
	}

	function selectConsumable(c: EquipmentSearchResult) {
		selectedConsumable = c;
		consumableSearch = c.name;
		consumableSearchResults = [];
	}

	function selectConsumableCustom(name: string) {
		const trimmed = name.trim();
		if (!trimmed) return;
		selectedConsumable = {
			catalogId: null,
			name: trimmed,
			decay: 0,
			ammoBurn: 0,
			isLimited: false,
		};
		consumableSearch = trimmed;
		consumableSearchResults = [];
	}

	async function toggleExpand(id: string) {
		if (expandedId === id) {
			expandedId = null;
			return;
		}
		expandedId = id;
		if (!detailCache[id]) {
			detailCache[id] = await getEquipmentDetail(id);
		}
	}

	function getDetail(id: string): EquipmentDetail | null {
		return detailCache[id] ?? null;
	}

	async function saveEquipment() {
		saving = true;
		try {
			if (addType === 'weapon') {
				if (!selectedWeapon?.catalogId) return;
				const payload = {
					type: 'weapon' as const,
					catalog_id: selectedWeapon.catalogId,
					amp_catalog_id: selectedAmp?.catalogId ?? null,
					scope_catalog_id: selectedScope?.catalogId ?? null,
					absorber_catalog_id: selectedAbsorber?.catalogId ?? null,
					weapon_markup: selectedWeapon.isLimited ? markupPercent : 100,
					amp_markup: selectedAmp?.isLimited ? markupPercent : 100,
					scope_markup: selectedScope?.isLimited ? scopeMarkupPercent : 100,
					absorber_markup: selectedAbsorber?.isLimited ? absorberMarkupPercent : 100,
					damage_enhancers: damageEnhancers,
				};
				const item = editingEquipmentId
					? await updateLibrary(editingEquipmentId, payload)
					: await addToLibrary(payload);
				replaceEquipment(item);
				detailCache[item.id] = await getEquipmentDetail(item.id);
			} else if (addType === 'healing') {
				if (!selectedHealer?.catalogId) return;
				const item = await addToLibrary({
					type: 'healing',
					catalog_id: selectedHealer.catalogId,
					weapon_markup: selectedHealer.isLimited ? markupPercent : 100,
				});
				replaceEquipment(item);
			} else {
				if (!selectedConsumable) return;
				const item = await addToLibrary({
					type: 'consumable',
					catalog_id: selectedConsumable.catalogId ?? null,
					name: selectedConsumable.catalogId ? null : selectedConsumable.name,
				});
				replaceEquipment(item);
			}
			showAddModal = false;
			editingEquipmentId = null;
		} catch (e) {
			pageError = e instanceof Error ? e.message : 'Failed to save equipment';
		} finally {
			saving = false;
		}
	}

	async function removeEquipment(id: string, type: 'weapon' | 'healing' | 'consumable' = 'weapon') {
		try {
			await removeFromLibrary(id);
			allEquipment = allEquipment.filter((e) => e.id !== id);
			if (type === 'healing') {
				healingTools = healingTools.filter((e) => e.id !== id);
			} else if (type === 'consumable') {
				consumables = consumables.filter((e) => e.id !== id);
			} else {
				equipmentList = equipmentList.filter((e) => e.id !== id);
			}
			if (expandedId === id) expandedId = null;
			delete detailCache[id];
		} catch (e) {
			pageError = e instanceof Error ? e.message : 'Failed to remove equipment';
		}
	}
</script>

{#if pageError}
	<div class="mx-6 mt-6">
		<Card class="p-3 flex items-center justify-between">
			<p class="text-sm text-error">{pageError}</p>
			<button type="button" class="linklet" onclick={() => (pageError = null)}>Dismiss</button>
		</Card>
	</div>
{/if}

<div class="px-6 pb-6 space-y-6">
	<!-- Page header -->
	<div class="flex items-center justify-between">
		<header class="flex flex-col gap-1.5">
			<h1 class="text-xl font-semibold text-text tracking-tight">Equipment</h1>
			<span class="block h-px w-12 bg-gradient-to-r from-accent/60 to-transparent"></span>
			<p class="text-sm text-text-secondary mt-0.5">
				Gear library with automatic cost-per-use calculation
			</p>
		</header>
		<div class="flex items-center gap-2">
			<button
				type="button"
				onclick={toggleSurfaceGuide}
				title={guideState.isActive ? 'Exit guide' : 'Open guide'}
				aria-label={guideState.isActive ? 'Exit guide' : 'Open guide for this page'}
				class="relative h-8 w-8 rounded-full border border-border bg-surface hover:bg-surface-hover text-text-secondary hover:text-text transition-colors flex items-center justify-center text-sm font-semibold {guideState.isActive ? 'z-[9100]' : ''}"
			>
				{#if guideState.isActive}
					<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="currentColor" class="w-3.5 h-3.5" aria-hidden="true">
						<path d="M5.28 4.22a.75.75 0 00-1.06 1.06L6.94 8l-2.72 2.72a.75.75 0 101.06 1.06L8 9.06l2.72 2.72a.75.75 0 101.06-1.06L9.06 8l2.72-2.72a.75.75 0 00-1.06-1.06L8 6.94 5.28 4.22z" />
					</svg>
				{:else}
					?
				{/if}
				{#if !guideSeen}
					<span class="absolute -top-0.5 -right-0.5 h-2 w-2 rounded-full bg-accent"></span>
				{/if}
			</button>
			{#if activeTab === 'library'}
				<Button size="sm" onclick={() => openAddModal()} data-guide-anchor="add-equipment-button">
					<svg
						xmlns="http://www.w3.org/2000/svg"
						viewBox="0 0 20 20"
						fill="currentColor"
						class="h-3.5 w-3.5"
					>
						<path
							d="M10.75 4.75a.75.75 0 00-1.5 0v4.5h-4.5a.75.75 0 000 1.5h4.5v4.5a.75.75 0 001.5 0v-4.5h4.5a.75.75 0 000-1.5h-4.5v-4.5z"
						/>
					</svg>
					Add Equipment
				</Button>
			{/if}
		</div>
	</div>

	<!-- Tabs -->
	<div data-guide-anchor="equipment-tabs">
		<Tabs {tabs} active={activeTab} onchange={(id) => (activeTab = id)} />
	</div>

	{#if activeTab === 'hotbar'}
		<HotbarTab
			equipment={allEquipment}
			{hotbar}
			enabled={guideState.isActive ? demoHotbarEnabled : true}
			onchange={(value: Hotbar) => {
				hotbar = { ...value };
			}}
		/>
	{:else if activeTab === 'trifecta'}
		<TrifectaTab
			equipment={allEquipment}
			{trifecta}
			enabled={guideState.isActive ? !demoHotbarEnabled : true}
			onchange={(value) => {
				trifecta = value;
			}}
		/>
	{:else}
	<!-- Equipment library -->
	<div class="mb-4 flex items-center gap-3">
		<h2 class="text-lg font-semibold text-text">Weapons</h2>
		<span class="text-text-tertiary" aria-hidden="true">
			<IconWeapons />
		</span>
	</div>

	{#if sortedEquipment.length === 0}
		<Card class="p-8">
			<div class="flex flex-col items-center text-center gap-3">
				<svg
					xmlns="http://www.w3.org/2000/svg"
					viewBox="0 0 24 24"
					fill="none"
					stroke="currentColor"
					stroke-width="1.5"
					class="h-10 w-10 text-text-tertiary"
				>
					<path
						stroke-linecap="round"
						stroke-linejoin="round"
						d="M11.42 15.17L17.25 21A2.652 2.652 0 0021 17.25l-5.877-5.877M11.42 15.17l2.496-3.03c.317-.384.74-.626 1.208-.766M11.42 15.17l-4.655 5.653a2.548 2.548 0 11-3.586-3.586l6.837-5.63m5.108-.233c.55-.164 1.163-.188 1.743-.14a4.5 4.5 0 004.486-6.336l-3.276 3.277a3.004 3.004 0 01-2.25-2.25l3.276-3.276a4.5 4.5 0 00-6.336 4.486c.091 1.076-.071 2.264-.904 2.95l-.102.085"
					/>
				</svg>
				<p class="text-sm text-text-secondary">Add your first weapon to enable automatic cost tracking.</p>
				<Button size="sm" onclick={() => openAddModal()}>Add Equipment</Button>
			</div>
		</Card>
	{:else}
		<div class="space-y-1">
			{#each sortedEquipment as item (item.id)}
				<!-- Equipment row -->
				<button
					data-guide-anchor="library-row-{item.id}"
					class="w-full text-left px-4 py-3 rounded-md transition-colors duration-[var(--duration-fast)]
						cursor-pointer
						{expandedId === item.id
						? 'bg-surface-hover'
						: 'hover:bg-surface-hover/50'}"
					onclick={() => toggleExpand(item.id)}
				>
					<div class="flex items-center gap-3">
						<!-- Type icon -->
						<div class="shrink-0 h-8 w-8 rounded-md bg-surface flex items-center justify-center">
							<div class="h-2 w-2 rounded-full bg-accent"></div>
						</div>

						<!-- Name + amp -->
						<div class="flex-1 min-w-0">
							<div class="flex items-center gap-2">
								<span class="text-sm font-medium text-text truncate">{item.name}</span>
							</div>
							{#if item.amplifierName}
								<p class="text-xs text-text-tertiary mt-0.5 truncate">
									+ {item.amplifierName}
								</p>
							{/if}
						</div>

						<!-- Cost -->
						<div class="text-right shrink-0">
							<span class="text-sm font-medium tabular-nums text-text">
								{formatPec(item.costPerUse)}
							</span>
							<span class="text-xs text-text-tertiary ml-0.5">PEC</span>
						</div>

						<!-- Enrichment badge -->
						<span data-guide-anchor="enrichment-badge-{item.id}" class="shrink-0">
							<Badge variant={enrichmentColor(item.enrichmentLevel)} class="shrink-0">
								{enrichmentLabel(item.enrichmentLevel)}
							</Badge>
						</span>

						<!-- Chevron -->
						<svg
							xmlns="http://www.w3.org/2000/svg"
							viewBox="0 0 20 20"
							fill="currentColor"
							class="h-4 w-4 text-text-tertiary transition-transform duration-[var(--duration-base)]
								{expandedId === item.id ? 'rotate-180' : ''}"
						>
							<path
								fill-rule="evenodd"
								d="M5.23 7.21a.75.75 0 011.06.02L10 11.168l3.71-3.938a.75.75 0 111.08 1.04l-4.25 4.5a.75.75 0 01-1.08 0l-4.25-4.5a.75.75 0 01.02-1.06z"
								clip-rule="evenodd"
							/>
						</svg>
					</div>
				</button>

				<!-- Inline detail panel -->
				{#if expandedId === item.id}
					{@const detail = getDetail(item.id)}
					{#if detail}
						<div class="ml-11 mr-4 mb-2 p-4 bg-surface rounded-md border border-border/50">
							<!-- Cost breakdown -->
							<h3 class="eyebrow mb-3">
								Cost Breakdown
							</h3>
							<div class="space-y-2 mb-4">
								{#each detail.costBreakdown as line}
									<div class="flex items-center justify-between text-sm">
										<span class="text-text-secondary">{line.component}</span>
										<div class="flex items-center gap-3 tabular-nums">
											<span class="text-text-tertiary text-xs">
												{formatPec(line.costPec)} PEC
												{#if line.markupMultiplier !== 1}
													<span class="text-warning">
														x{line.markupMultiplier.toFixed(2)}
													</span>
												{/if}
											</span>
											<span class="text-text font-medium w-16 text-right">
												{formatPec(line.effectiveCostPec)}
											</span>
										</div>
									</div>
								{/each}
								<Divider />
								<div class="flex items-center justify-between text-sm font-medium">
									<span class="text-text">Total per use</span>
									<span class="text-accent tabular-nums">
										{formatPec(detail.totalCostPerUse)} PEC
									</span>
								</div>
							</div>

							<!-- Component list -->
							<h3 class="eyebrow mb-2">
								Components
							</h3>
							<div class="space-y-1.5 text-sm mb-4">
								<div class="flex items-center justify-between">
									<span class="text-text">
										{detail.weapon.name}
									</span>
									<span class="text-text-secondary text-xs tabular-nums">
										Decay {formatPec(detail.weapon.decay)} · Ammo {formatPec(detail.weapon.ammoBurn)} PEC
									</span>
								</div>
								{#if detail.weapon.damageEnhancers > 0}
									<div class="flex items-center justify-between">
										<span class="text-text">Damage enhancers</span>
										<span class="text-text-secondary text-xs tabular-nums">
											{detail.weapon.damageEnhancers} slot{detail.weapon.damageEnhancers === 1 ? '' : 's'}
										</span>
									</div>
								{/if}
								{#if detail.amplifier}
									<div class="flex items-center justify-between">
										<span class="text-text">
											{detail.amplifier.name}
										</span>
										<span class="text-text-secondary text-xs tabular-nums">
											Decay {formatPec(detail.amplifier.decay)} · Ammo
											{formatPec(detail.amplifier.ammoBurn)} PEC
										</span>
									</div>
								{/if}
								{#if detail.scope}
									<div class="flex items-center justify-between">
										<span class="text-text">
											{detail.scope.name}
										</span>
										<span class="text-text-secondary text-xs tabular-nums">
											Decay {formatPec(detail.scope.decay)}
											{#if detail.scope.markupPercent !== 100}
												· {detail.scope.markupPercent}%
											{/if}
										</span>
									</div>
								{/if}
								{#if detail.absorber}
									<div class="flex items-center justify-between">
										<span class="text-text">
											{detail.absorber.name}
										</span>
										<span class="text-text-secondary text-xs tabular-nums">
											-{detail.absorber.absorptionPercent}% weapon decay
											{#if detail.absorber.markupPercent !== 100}
												· {detail.absorber.markupPercent}%
											{/if}
										</span>
									</div>
								{/if}
							</div>

							<!-- Actions -->
							<div class="flex items-center gap-2">
								<Button size="sm" variant="ghost" onclick={() => openEditModal(item.id)}>
									Edit
								</Button>
								<Button size="sm" variant="danger" onclick={() => removeEquipment(item.id)}>
									Remove
								</Button>
							</div>
						</div>
					{:else}
						<!-- Loading detail -->
						<div class="ml-11 mr-4 mb-2 p-4 bg-surface rounded-md border border-border/50">
							<p class="text-xs text-text-tertiary">Loading…</p>
						</div>
					{/if}
				{/if}
			{/each}
		</div>
	{/if}

	<!-- Consumables section -->
	<Divider />
	<div>
		<div class="mb-4 flex items-center gap-3">
			<h2 class="text-lg font-semibold text-text">Consumables</h2>
			<span class="text-text-tertiary" aria-hidden="true">
				<IconConsumables />
			</span>
		</div>

		{#if consumables.length === 0}
			<p class="text-sm text-text-tertiary py-4">
				No consumables configured.
			</p>
		{:else}
			<div class="space-y-1">
				{#each consumables as item (item.id)}
					<div
						class="flex items-center gap-3 px-4 py-3 rounded-md hover:bg-surface-hover/50
							transition-colors duration-[var(--duration-fast)]"
					>
						<div class="shrink-0 h-8 w-8 rounded-md bg-surface flex items-center justify-center">
							<div class="h-2 w-2 rounded-full bg-warning"></div>
						</div>
						<div class="flex-1 min-w-0">
							<span class="text-sm font-medium text-text">{item.name}</span>
						</div>
						<button
							type="button" class="linklet linklet-danger shrink-0"
							onclick={() => removeEquipment(item.id, 'consumable')}
							title="Remove"
						>
							<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="currentColor" class="w-3.5 h-3.5">
								<path d="M5.28 4.22a.75.75 0 00-1.06 1.06L6.94 8l-2.72 2.72a.75.75 0 101.06 1.06L8 9.06l2.72 2.72a.75.75 0 101.06-1.06L9.06 8l2.72-2.72a.75.75 0 00-1.06-1.06L8 6.94 5.28 4.22z" />
							</svg>
						</button>
					</div>
				{/each}
			</div>
		{/if}
	</div>

	<!-- Healing tools section -->
	<Divider />
	<div>
		<div class="mb-4 flex items-center gap-3">
			<h2 class="text-lg font-semibold text-text">Healing Tools</h2>
			<span class="text-text-tertiary" aria-hidden="true">
				<IconHealing />
			</span>
		</div>

		{#if healingTools.length === 0}
			<p class="text-sm text-text-tertiary py-4">
				No healing tools configured. They'll appear here when detected during tracking.
			</p>
		{:else}
			<div class="space-y-1">
				{#each healingTools as tool (tool.id)}
					<div
						class="flex items-center gap-3 px-4 py-3 rounded-md hover:bg-surface-hover/50
							transition-colors duration-[var(--duration-fast)]"
					>
						<div class="shrink-0 h-8 w-8 rounded-md bg-surface flex items-center justify-center">
							<div class="h-2 w-2 rounded-full bg-positive"></div>
						</div>
						<div class="flex-1 min-w-0">
							<div class="flex items-center gap-2">
								<span class="text-sm font-medium text-text">{tool.name}</span>
							</div>
						</div>
						<div class="text-right shrink-0">
							<span class="text-sm font-medium tabular-nums text-text">
								{formatPec(tool.costPerHeal)}
							</span>
							<span class="text-xs text-text-tertiary ml-0.5">PEC/heal</span>
						</div>
						<button
							type="button" class="linklet linklet-danger shrink-0"
							onclick={() => removeEquipment(tool.id, 'healing')}
							title="Remove"
						>
							<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="currentColor" class="w-3.5 h-3.5">
								<path d="M5.28 4.22a.75.75 0 00-1.06 1.06L6.94 8l-2.72 2.72a.75.75 0 101.06 1.06L8 9.06l2.72 2.72a.75.75 0 101.06-1.06L9.06 8l2.72-2.72a.75.75 0 00-1.06-1.06L8 6.94 5.28 4.22z" />
							</svg>
						</button>
					</div>
				{/each}
			</div>
		{/if}
	</div>

	{/if}
</div>

<!-- Add Equipment Modal -->
<Modal bind:open={showAddModal} title={editingEquipmentId ? 'Edit Equipment' : 'Add Equipment'} class="max-w-lg">
	<div class="space-y-5">
		<!-- Type toggle -->
		{#if !editingEquipmentId}
			<SegmentedControl
				size="md"
				options={[
					{ id: 'weapon', label: 'Weapon' },
					{ id: 'healing', label: 'Healing Tool' },
					{ id: 'consumable', label: 'Consumable' }
				]}
				active={addType}
				onchange={(id) => {
					addType = id as 'weapon' | 'healing' | 'consumable';
					if (id === 'weapon') {
						selectedHealer = null;
						healerSearch = '';
					} else if (id === 'healing') {
						selectedWeapon = null;
						weaponSearch = '';
						selectedAmp = null;
						ampSearch = '';
					} else {
						selectedWeapon = null;
						weaponSearch = '';
						selectedAmp = null;
						ampSearch = '';
						selectedHealer = null;
						healerSearch = '';
					}
				}}
			/>
		{/if}

		{#if addType === 'weapon'}
			<!-- Weapon selection -->
			<div>
				<label for="equipment-weapon-search" class="block eyebrow mb-1.5">
					Weapon
				</label>
				<SearchInput id="equipment-weapon-search" bind:value={weaponSearch} placeholder="Search weapons…" />
				{#if weaponSearchResults.length > 0 && !selectedWeapon}
					<div class="mt-1 bg-surface border border-border rounded-md overflow-hidden max-h-48 overflow-y-auto">
						{#each weaponSearchResults as result}
							<button
								class="w-full text-left px-3 py-2 text-sm hover:bg-surface-hover
									transition-colors duration-[var(--duration-fast)] cursor-pointer
									flex items-center justify-between"
								onclick={() => selectWeapon(result)}
							>
								<span class="text-text">
									{result.name}
								</span>
								<span class="text-xs text-text-tertiary tabular-nums">
									D:{result.decay.toFixed(3)} A:{result.ammoBurn.toFixed(2)} PEC
								</span>
							</button>
						{/each}
					</div>
				{/if}
				{#if selectedWeapon}
					<div class="mt-2 px-3 py-2 bg-surface rounded-md border border-border/50 text-sm">
						<div class="flex items-center justify-between">
							<span class="text-text font-medium">{selectedWeapon.name}</span>
							<button type="button" class="linklet"
								onclick={() => { selectedWeapon = null; weaponSearch = ''; }}>Change</button>
						</div>
						<div class="flex gap-4 mt-1 text-xs text-text-secondary tabular-nums">
							<span>Decay: {selectedWeapon.decay.toFixed(3)} PEC</span>
							<span>Ammo: {selectedWeapon.ammoBurn.toFixed(2)} PEC/shot</span>
						</div>
					</div>
				{/if}
			</div>

			<!-- Amplifier (optional) -->
			<div>
				<label for="equipment-amp-search" class="block eyebrow mb-1.5">
					Amplifier <span class="font-normal text-text-tertiary">(optional)</span>
				</label>
				<SearchInput id="equipment-amp-search" bind:value={ampSearch} placeholder="Search amplifiers…" />
				{#if ampSearchResults.length > 0 && !selectedAmp}
					<div class="mt-1 bg-surface border border-border rounded-md overflow-hidden max-h-36 overflow-y-auto">
						{#each ampSearchResults as result}
							<button
								class="w-full text-left px-3 py-2 text-sm hover:bg-surface-hover
									transition-colors duration-[var(--duration-fast)] cursor-pointer
									flex items-center justify-between"
								onclick={() => selectAmp(result)}
							>
								<span class="text-text">
									{result.name}
								</span>
								<span class="text-xs text-text-tertiary tabular-nums">
									D:{result.decay.toFixed(3)} A:{result.ammoBurn.toFixed(2)} PEC
								</span>
							</button>
						{/each}
					</div>
				{/if}
				{#if selectedAmp}
					<div class="mt-2 px-3 py-2 bg-surface rounded-md border border-border/50 text-sm">
						<div class="flex items-center justify-between">
							<span class="text-text font-medium">{selectedAmp.name}</span>
							<button type="button" class="linklet"
								onclick={() => { selectedAmp = null; ampSearch = ''; }}>Remove</button>
						</div>
						<div class="flex gap-4 mt-1 text-xs text-text-secondary tabular-nums">
							<span>Decay: {selectedAmp.decay.toFixed(3)} PEC</span>
							<span>Ammo: {selectedAmp.ammoBurn.toFixed(2)} PEC/shot</span>
						</div>
					</div>
				{/if}
			</div>

			<!-- Optional attachments -->
			<div>
				<button
					data-guide-anchor="optional-attachments-toggle"
					class="flex items-center gap-1.5 text-xs text-text-secondary hover:text-text
						transition-colors duration-[var(--duration-fast)] cursor-pointer"
					onclick={() => (showOptionalAttachments = !showOptionalAttachments)}
				>
					<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor"
						class="h-3.5 w-3.5 transition-transform duration-[var(--duration-base)]
							{showOptionalAttachments ? 'rotate-180' : ''}">
						<path fill-rule="evenodd" d="M5.23 7.21a.75.75 0 011.06.02L10 11.168l3.71-3.938a.75.75 0 111.08 1.04l-4.25 4.5a.75.75 0 01-1.08 0l-4.25-4.5a.75.75 0 01.02-1.06z" clip-rule="evenodd" />
					</svg>
					Optional attachments (scope, absorber)
				</button>
				{#if showOptionalAttachments}
					<div class="mt-3 pl-4 space-y-4 border-l border-border">
						<!-- Scope -->
						<div>
							<label for="equipment-scope-search" class="block eyebrow mb-1.5">
								Scope
							</label>
							<SearchInput id="equipment-scope-search" bind:value={scopeSearch} placeholder="Search scopes…" />
							{#if scopeSearchResults.length > 0 && !selectedScope}
								<div class="mt-1 bg-surface border border-border rounded-md overflow-hidden max-h-36 overflow-y-auto">
									{#each scopeSearchResults as result}
										<button
											class="w-full text-left px-3 py-2 text-sm hover:bg-surface-hover
												transition-colors duration-[var(--duration-fast)] cursor-pointer
												flex items-center justify-between"
											onclick={() => selectScope(result)}
										>
											<span class="text-text">
												{result.name}
											</span>
											<span class="text-xs text-text-tertiary tabular-nums">
												D:{result.decay.toFixed(3)} PEC
											</span>
										</button>
									{/each}
								</div>
							{/if}
							{#if selectedScope}
								<div class="mt-2 px-3 py-2 bg-surface rounded-md border border-border/50 text-sm">
									<div class="flex items-center justify-between">
										<span class="text-text font-medium">{selectedScope.name}</span>
										<button type="button" class="linklet"
											onclick={() => { selectedScope = null; scopeSearch = ''; }}>Remove</button>
									</div>
									<div class="flex gap-4 mt-1 text-xs text-text-secondary tabular-nums">
										<span>Decay: {selectedScope.decay.toFixed(3)} PEC</span>
									</div>
								</div>
								{#if selectedScope.isLimited}
									<div class="mt-1.5 flex items-center gap-2">
										<label for="equipment-scope-markup" class="text-xs text-text-tertiary">Scope markup %</label>
										<Input id="equipment-scope-markup" type="number" bind:value={scopeMarkupPercent} min={100} max={10000} class="w-20" />
									</div>
								{/if}
							{/if}
						</div>

						<!-- Absorber -->
						<div>
							<label for="equipment-absorber-search" class="block eyebrow mb-1.5">
								Absorber
							</label>
							<SearchInput id="equipment-absorber-search" bind:value={absorberSearch} placeholder="Search absorbers…" />
							{#if absorberSearchResults.length > 0 && !selectedAbsorber}
								<div class="mt-1 bg-surface border border-border rounded-md overflow-hidden max-h-36 overflow-y-auto">
									{#each absorberSearchResults as result}
										<button
											class="w-full text-left px-3 py-2 text-sm hover:bg-surface-hover
												transition-colors duration-[var(--duration-fast)] cursor-pointer
												flex items-center justify-between"
											onclick={() => selectAbsorber(result)}
										>
											<span class="text-text">
												{result.name}
											</span>
										</button>
									{/each}
								</div>
							{/if}
							{#if selectedAbsorber}
								<div class="mt-2 px-3 py-2 bg-surface rounded-md border border-border/50 text-sm">
									<div class="flex items-center justify-between">
										<span class="text-text font-medium">{selectedAbsorber.name}</span>
										<button type="button" class="linklet"
											onclick={() => { selectedAbsorber = null; absorberSearch = ''; }}>Remove</button>
									</div>
								</div>
								{#if selectedAbsorber.isLimited}
									<div class="mt-1.5 flex items-center gap-2">
										<label for="equipment-absorber-markup" class="text-xs text-text-tertiary">Absorber markup %</label>
										<Input id="equipment-absorber-markup" type="number" bind:value={absorberMarkupPercent} min={100} max={10000} class="w-20" />
									</div>
								{/if}
							{/if}
						</div>
					</div>
				{/if}
			</div>

			<div>
				<label for="equipment-damage-enhancers" class="block eyebrow mb-1.5">
					Damage enhancers
				</label>
				<Input id="equipment-damage-enhancers" type="number" bind:value={damageEnhancers} min={0} class="w-24" />
				<p class="text-xs text-text-tertiary mt-1">
					Configured slots on this weapon. Each slot is treated as a full stack at session start.
				</p>
			</div>

			<!-- Live cost preview -->
			{#if liveCostPreview !== null}
				<div class="p-3 bg-accent-faint rounded-md border border-accent/20">
					<div class="flex items-center justify-between">
						<span class="eyebrow">Estimated cost per use</span>
						<span class="text-lg font-semibold tabular-nums text-accent">{formatPec(liveCostPreview)} PEC</span>
					</div>
				</div>
			{/if}
		{:else if addType === 'healing'}
			<!-- Healing tool selection -->
			<div>
				<label for="equipment-healer-search" class="block eyebrow mb-1.5">
					Healing Tool
				</label>
				<SearchInput id="equipment-healer-search" bind:value={healerSearch} placeholder="Search medical tools…" />
				{#if healerSearchResults.length > 0 && !selectedHealer}
					<div class="mt-1 bg-surface border border-border rounded-md overflow-hidden max-h-48 overflow-y-auto">
						{#each healerSearchResults as result}
							<button
								class="w-full text-left px-3 py-2 text-sm hover:bg-surface-hover
									transition-colors duration-[var(--duration-fast)] cursor-pointer
									flex items-center justify-between"
								onclick={() => selectHealer(result)}
							>
								<span class="text-text">
									{result.name}
								</span>
								<span class="text-xs text-text-tertiary tabular-nums">
									D:{result.decay.toFixed(3)} A:{result.ammoBurn.toFixed(2)} PEC
								</span>
							</button>
						{/each}
					</div>
				{/if}
				{#if selectedHealer}
					<div class="mt-2 px-3 py-2 bg-surface rounded-md border border-border/50 text-sm">
						<div class="flex items-center justify-between">
							<span class="text-text font-medium">{selectedHealer.name}</span>
							<button type="button" class="linklet"
								onclick={() => { selectedHealer = null; healerSearch = ''; }}>Change</button>
						</div>
						<div class="flex gap-4 mt-1 text-xs text-text-secondary tabular-nums">
							<span>Decay: {selectedHealer.decay.toFixed(3)} PEC</span>
							<span>Ammo: {selectedHealer.ammoBurn.toFixed(2)} PEC/use</span>
						</div>
					</div>
				{/if}
			</div>
		{:else if addType === 'consumable'}
			<!-- Consumable selection -->
			<div>
				<label for="equipment-consumable-search" class="block eyebrow mb-1.5">
					Consumable
				</label>
				<SearchInput id="equipment-consumable-search" bind:value={consumableSearch} placeholder="Search or type a custom name…" />
				{#if !selectedConsumable && consumableSearch.trim().length >= 2}
					<div class="mt-1 bg-surface border border-border rounded-md overflow-hidden max-h-48 overflow-y-auto">
						{#each consumableSearchResults as result}
							<button
								class="w-full text-left px-3 py-2 text-sm hover:bg-surface-hover
									transition-colors duration-[var(--duration-fast)] cursor-pointer"
								onclick={() => selectConsumable(result)}
							>
								<span class="text-text">{result.name}</span>
							</button>
						{/each}
						{#if !consumableSearchResults.some((r) => r.name.toLowerCase() === consumableSearch.trim().toLowerCase())}
							<button
								class="w-full text-left px-3 py-2 text-sm hover:bg-surface-hover
									transition-colors duration-[var(--duration-fast)] cursor-pointer
									border-t border-border/50"
								onclick={() => selectConsumableCustom(consumableSearch)}
							>
								<span class="text-text-secondary">Add custom: </span>
								<span class="text-text font-medium">{consumableSearch.trim()}</span>
							</button>
						{/if}
					</div>
				{/if}
				{#if selectedConsumable}
					<div class="mt-2 px-3 py-2 bg-surface rounded-md border border-border/50 text-sm">
						<div class="flex items-center justify-between">
							<span class="text-text font-medium">{selectedConsumable.name}</span>
							<button type="button" class="linklet"
								onclick={() => { selectedConsumable = null; consumableSearch = ''; }}>Change</button>
						</div>
						{#if !selectedConsumable.catalogId}
							<div class="mt-1 text-xs text-text-tertiary">Custom entry</div>
						{/if}
					</div>
				{/if}
			</div>
		{/if}

		<!-- Markup (conditional on limited items — applies to both types) -->
		{#if (addType === 'weapon' && (selectedWeapon?.isLimited || selectedAmp?.isLimited)) || (addType === 'healing' && selectedHealer?.isLimited)}
			<div>
				<label for="equipment-item-markup" class="block eyebrow mb-1.5">
					Item Markup %
				</label>
				<Input id="equipment-item-markup" type="number" bind:value={markupPercent} min={100} max={10000} class="w-24" />
				<p class="text-xs text-text-tertiary mt-1">
					Replacement cost for limited items. 200% means each PEC of decay costs 2 PEC to replace.
				</p>
			</div>
		{/if}

		<!-- Actions -->
		<div class="flex items-center justify-end gap-2 pt-2">
			<Button variant="ghost" onclick={() => (showAddModal = false)}>Cancel</Button>
			<Button
				disabled={(addType === 'weapon' ? !selectedWeapon : addType === 'healing' ? !selectedHealer : !selectedConsumable) || saving}
				onclick={saveEquipment}
			>
				{saving ? 'Saving…' : editingEquipmentId ? 'Save Changes' : 'Save'}
			</Button>
		</div>
	</div>
</Modal>
