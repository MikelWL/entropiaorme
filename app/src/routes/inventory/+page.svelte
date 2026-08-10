<script lang="ts">
	import { onMount } from 'svelte';
	import type { InventoryItem } from '$lib/api';
	import ErrorNotice from '$lib/components/ErrorNotice.svelte';
	import SegmentedControl from '$lib/components/SegmentedControl.svelte';
	import Tabs from '$lib/components/Tabs.svelte';
	import ActivityHistory from '$lib/features/analytics/ActivityHistory.svelte';
	import AdjustStockModal from '$lib/features/analytics/AdjustStockModal.svelte';
	import AuctionListings from '$lib/features/analytics/AuctionListings.svelte';
	import ConvertStockModal from '$lib/features/analytics/ConvertStockModal.svelte';
	import SellStockModal from '$lib/features/analytics/SellStockModal.svelte';
	import EquipmentSaleModal from '$lib/features/inventory/EquipmentSaleModal.svelte';
	import InventoryHoldings from '$lib/features/inventory/InventoryHoldings.svelte';
	import InventoryItemFormModal from '$lib/features/inventory/InventoryItemFormModal.svelte';
	import {
		createInventoryModel,
		type InventoryKind,
		type InventoryView,
	} from '$lib/features/inventory/inventoryModel.svelte';
	import type {
		ConfidenceMode,
		TreeCuttingStock,
	} from '$lib/features/analytics/treeCuttingModel.svelte';

	const model = createInventoryModel();
	let equipmentFormOpen = $state(false);
	let equipmentToEdit = $state<InventoryItem | null>(null);
	let equipmentToSell = $state<InventoryItem | null>(null);
	let lootToSell = $state<TreeCuttingStock | null>(null);
	let lootToConvert = $state<TreeCuttingStock | null>(null);
	let lootToRemove = $state<TreeCuttingStock | null>(null);
	let shrapnelToConvert = $state<TreeCuttingStock | null>(null);

	const kindOptions = [
		{ id: 'loot', label: 'Loot' },
		{ id: 'equipment', label: 'Assets' },
	];
	const tabs = [
		{ id: 'holdings', label: 'Assets' },
		{ id: 'listings', label: 'Listings' },
		{ id: 'history', label: 'History' },
	];

	onMount(() => void model.load());

	function selectView(id: string) {
		model.view = id as InventoryView;
		if (id === 'history' && model.history.length === 0) void model.loadHistory();
	}

	function addEquipment() {
		equipmentToEdit = null;
		equipmentFormOpen = true;
	}

	function editEquipment(item: InventoryItem) {
		equipmentToEdit = item;
		equipmentFormOpen = true;
	}

	async function deleteEquipment(item: InventoryItem) {
		if (!window.confirm(`Remove ${item.name} from your assets?`)) return;
		await model.deleteEquipment(item);
	}
</script>

<div class="flex h-full min-h-0 flex-col gap-6 px-6 pb-6">
	<header class="shrink-0">
		<div class="flex flex-col gap-1.5">
			<h1 class="text-xl font-semibold tracking-tight text-text">Inventory</h1>
			<span class="block h-px w-12 bg-gradient-to-r from-accent/60 to-transparent"></span>
			<p class="mt-0.5 text-sm text-text-secondary">
				Manage your loot and assets; review activity performance in Analytics.
			</p>
		</div>
	</header>

	<Tabs {tabs} active={model.view} onchange={selectView} class="shrink-0" />

	<ErrorNotice message={model.error} />

	<div class="min-h-0 flex-1">
		{#if model.loading}
			<p class="py-16 text-center text-sm text-text-tertiary">Reading inventory...</p>
		{:else if model.view === 'holdings'}
			<InventoryHoldings
			kind={model.kind}
			onkindchange={(id) => (model.kind = id as InventoryKind)}
			confidenceMode={model.confidenceMode}
			onconfidencechange={(id) => (model.confidenceMode = id as ConfidenceMode)}
			loot={model.loot}
			equipment={model.equipment}
			onsellloot={(item) => (lootToSell = item)}
			onconvert={(item) => (lootToConvert = item)}
			onremove={(item) => (lootToRemove = item)}
			onshrapnel={(item) => (shrapnelToConvert = item)}
			onaddequipment={addEquipment}
			oneditequipment={editEquipment}
			onsellequipment={(item) => (equipmentToSell = item)}
			ondeleteequipment={deleteEquipment}
			/>
		{:else if model.view === 'listings'}
			<div class="py-3">
			<SegmentedControl
				options={kindOptions}
				active={model.kind}
				size="md"
				onchange={(id) => (model.kind = id as InventoryKind)}
			/>
			</div>
			<AuctionListings
			open={model.openListings}
			resolved={model.resolvedListings}
			onresolve={model.resolveListing}
			central
			embedded
			/>
		{:else}
			<div class="py-3">
			<SegmentedControl
				options={kindOptions}
				active={model.kind}
				size="md"
				onchange={(id) => (model.kind = id as InventoryKind)}
			/>
			</div>
			<ActivityHistory
			entries={model.history}
			loading={model.historyLoading}
			onundo={model.undo}
			embedded
			/>
		{/if}
	</div>
</div>

<SellStockModal
	item={lootToSell}
	onlist={model.listLoot}
	ontrade={model.sellLootByTrade}
	oncancel={() => (lootToSell = null)}
	activityAttributionNoun="its source activities"
/>
<ConvertStockModal
	item={lootToConvert}
	onconvert={model.recycle}
	oncancel={() => (lootToConvert = null)}
/>
<AdjustStockModal
	item={lootToRemove}
	mode="remove"
	onconfirm={model.remove}
	oncancel={() => (lootToRemove = null)}
/>
<AdjustStockModal
	item={shrapnelToConvert}
	mode="shrapnel"
	onconfirm={(_itemName, quantity) => model.shrapnel(quantity)}
	oncancel={() => (shrapnelToConvert = null)}
/>
<InventoryItemFormModal
	bind:open={equipmentFormOpen}
	item={equipmentToEdit}
	onsaved={() => void model.refresh()}
/>
<EquipmentSaleModal
	item={equipmentToSell}
	onlist={model.listEquipment}
	ontrade={model.sellEquipmentByTrade}
	oncancel={() => (equipmentToSell = null)}
/>
