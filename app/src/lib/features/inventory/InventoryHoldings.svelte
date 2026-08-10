<script lang="ts">
	import Button from '$lib/components/Button.svelte';
	import SearchInput from '$lib/components/SearchInput.svelte';
	import SegmentedControl from '$lib/components/SegmentedControl.svelte';
	import type { InventoryItem } from '$lib/api';
	import MarkupConfidenceControl from '$lib/features/analytics/MarkupConfidenceControl.svelte';
	import TreeCuttingStockView from '$lib/features/analytics/TreeCuttingStock.svelte';
	import type {
		ConfidenceMode,
		TreeCuttingStock,
	} from '$lib/features/analytics/treeCuttingModel.svelte';
	import { formatLedgerDate, formatPed } from '$lib/utils/format';
	import type { InventoryKind } from './inventoryModel.svelte';

	let {
		kind,
		onkindchange,
		confidenceMode,
		onconfidencechange,
		loot,
		equipment,
		onsellloot,
		onconvert,
		onremove,
		onshrapnel,
		onaddequipment,
		oneditequipment,
		onsellequipment,
		ondeleteequipment,
	}: {
		kind: InventoryKind;
		onkindchange: (id: string) => void;
		confidenceMode: ConfidenceMode;
		onconfidencechange: (id: string) => void;
		loot: TreeCuttingStock[];
		equipment: InventoryItem[];
		onsellloot: (item: TreeCuttingStock) => void;
		onconvert: (item: TreeCuttingStock) => void;
		onremove: (item: TreeCuttingStock) => void;
		onshrapnel: (item: TreeCuttingStock) => void;
		onaddequipment: () => void;
		oneditequipment: (item: InventoryItem) => void;
		onsellequipment: (item: InventoryItem) => void;
		ondeleteequipment: (item: InventoryItem) => void;
	} = $props();

	let query = $state('');
	const normalisedQuery = $derived(query.trim().toLowerCase());
	const visibleEquipment = $derived(
		equipment.filter((item) => !normalisedQuery || item.name.toLowerCase().includes(normalisedQuery)),
	);
	const kindOptions = [
		{ id: 'loot', label: 'Loot' },
		{ id: 'equipment', label: 'Equipment' },
	];
</script>

<section
	aria-label={kind === 'loot' ? 'Loot holdings' : 'Equipment holdings'}
	class="flex h-full min-h-0 flex-col"
>
	{#if kind === 'loot'}
		<TreeCuttingStockView
			stock={loot}
			onsell={onsellloot}
			onconvert={onconvert}
			onremove={onremove}
			onshrapnelconvert={onshrapnel}
			actionLayout="inventory"
			alwaysSearch
			fillAvailable
			emptyMessage="No tracked loot is currently held."
			sourceDescription="Loot pooled across every tracked profession, minus stock you have sold, converted, or removed."
		>
			{#snippet heading()}
				<SegmentedControl
					options={kindOptions}
					active={kind}
					size="md"
					onchange={onkindchange}
				/>
			{/snippet}
			{#snippet controls()}
				<MarkupConfidenceControl active={confidenceMode} onchange={onconfidencechange} />
			{/snippet}
		</TreeCuttingStockView>
	{:else}
		<div class="flex shrink-0 flex-wrap items-center justify-between gap-3 py-3">
			<SegmentedControl
				options={kindOptions}
				active={kind}
				size="md"
				onchange={onkindchange}
			/>
			<div class="flex items-center gap-2">
				<SearchInput
					class="w-56"
					bind:value={query}
					placeholder="Find a holding"
					aria-label="Find a holding"
				/>
				<Button size="sm" onclick={onaddequipment}>Add holding</Button>
			</div>
		</div>

		{#if visibleEquipment.length === 0}
			<div class="px-6 py-12 text-center">
				<p class="text-sm text-text-tertiary">
					{query ? 'No equipment holding matches that search.' : 'No equipment holdings recorded yet.'}
				</p>
				{#if !query}
					<Button class="mt-4" size="sm" onclick={onaddequipment}>Add your first holding</Button>
				{/if}
			</div>
		{:else}
			<div class="min-h-0 flex-1 overflow-auto border-t border-border/50">
				<table class="w-full min-w-[48rem] text-sm">
					<thead class="sticky top-0 z-10 bg-surface">
						<tr class="border-b border-border">
							<th class="px-4 py-2 text-left eyebrow">Holding</th>
							<th class="px-3 py-2 text-right eyebrow">TT</th>
							<th class="px-3 py-2 text-right eyebrow">Markup paid</th>
							<th class="px-3 py-2 text-right eyebrow">Cost basis</th>
							<th class="px-3 py-2 text-right eyebrow">Acquired</th>
							<th class="px-4 py-2 text-right eyebrow">Actions</th>
						</tr>
					</thead>
					<tbody>
						{#each visibleEquipment as item (item.id)}
							<tr class="border-b border-border/40 last:border-b-0 hover:bg-surface-hover/40">
								<td class="max-w-64 px-4 py-3">
									<p class="truncate font-medium text-text">{item.name}</p>
									{#if item.notes}<p class="mt-0.5 truncate text-xs text-text-tertiary">{item.notes}</p>{/if}
								</td>
								<td class="px-3 py-3 text-right tabular-nums text-text-secondary">{formatPed(item.ttValue)}</td>
								<td class="px-3 py-3 text-right tabular-nums text-text-secondary">{formatPed(item.markupPaid)}</td>
								<td class="px-3 py-3 text-right tabular-nums font-medium text-text">{formatPed(item.ttValue + item.markupPaid)}</td>
								<td class="px-3 py-3 text-right text-xs tabular-nums text-text-tertiary">{formatLedgerDate(item.acquiredAt)}</td>
								<td class="px-4 py-3">
									<div class="flex items-center justify-end gap-1.5">
										<Button size="sm" variant="ghost" onclick={() => oneditequipment(item)}>Edit</Button>
										<Button size="sm" onclick={() => onsellequipment(item)}>Sell</Button>
										<Button size="sm" variant="ghost" onclick={() => ondeleteequipment(item)}>Remove</Button>
									</div>
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>
		{/if}
	{/if}
</section>
