<script lang="ts">
	import Button from '$lib/components/Button.svelte';
	import Card from '$lib/components/Card.svelte';
	import SearchInput from '$lib/components/SearchInput.svelte';
	import type { InventoryItem } from '$lib/api';
	import type { TreeCuttingStock } from '$lib/features/analytics/treeCuttingModel.svelte';
	import { formatLedgerDate, formatPed } from '$lib/utils/format';
	import type { InventoryKind } from './inventoryModel.svelte';

	let {
		kind,
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
	const visibleLoot = $derived(
		loot.filter((item) => !normalisedQuery || item.itemName.toLowerCase().includes(normalisedQuery)),
	);
	const visibleEquipment = $derived(
		equipment.filter((item) => !normalisedQuery || item.name.toLowerCase().includes(normalisedQuery)),
	);
</script>

<Card class="overflow-hidden">
	<div class="flex flex-wrap items-center justify-between gap-3 border-b border-border/50 px-4 py-3">
		<div>
			<h2 class="text-sm font-semibold tracking-tight text-text">
				{kind === 'loot' ? 'Loot holdings' : 'Equipment holdings'}
			</h2>
			<p class="mt-0.5 text-xs text-text-tertiary">
				{kind === 'loot'
					? 'Canonical items pooled across every tracked profession.'
					: 'Whole capital positions with their original acquisition basis.'}
			</p>
		</div>
		<div class="flex items-center gap-2">
			<SearchInput
				class="w-56"
				bind:value={query}
				placeholder="Find a holding"
				aria-label="Find a holding"
			/>
			{#if kind === 'equipment'}
				<Button size="sm" onclick={onaddequipment}>Add holding</Button>
			{/if}
		</div>
	</div>

	{#if kind === 'loot'}
		{#if visibleLoot.length === 0}
			<p class="px-6 py-12 text-center text-sm text-text-tertiary">
				{query ? 'No loot holding matches that search.' : 'No tracked loot is currently held.'}
			</p>
		{:else}
			<div class="max-h-[34rem] overflow-auto">
				<table class="w-full min-w-[46rem] text-sm">
					<thead class="sticky top-0 z-10 bg-surface">
						<tr class="border-b border-border">
							<th class="px-4 py-2 text-left eyebrow">Item</th>
							<th class="px-3 py-2 text-right eyebrow">Held</th>
							<th class="px-3 py-2 text-right eyebrow">TT</th>
							<th class="px-3 py-2 text-right eyebrow">Listed</th>
							<th class="px-4 py-2 text-right eyebrow">Actions</th>
						</tr>
					</thead>
					<tbody>
						{#each visibleLoot as item (item.itemName)}
							<tr class="border-b border-border/40 last:border-b-0 hover:bg-surface-hover/40">
								<td class="px-4 py-3 font-medium text-text">{item.itemName}</td>
								<td class="px-3 py-3 text-right tabular-nums text-text-secondary">{item.heldQty}</td>
								<td class="px-3 py-3 text-right tabular-nums text-text">{formatPed(item.heldTt)}</td>
								<td class="px-3 py-3 text-right tabular-nums text-text-secondary">
									{item.listedQty > 0 ? item.listedQty : '—'}
								</td>
								<td class="px-4 py-3">
									<div class="flex items-center justify-end gap-1.5">
										<Button size="sm" onclick={() => onsellloot(item)} disabled={item.heldQty <= 0}>Sell</Button>
										{#if item.itemName === 'Shrapnel'}
											<Button size="sm" variant="ghost" onclick={() => onshrapnel(item)} disabled={item.heldQty <= 0}>Convert 101%</Button>
										{:else if item.itemName !== 'Nanocube' && item.itemName !== 'Universal Ammo'}
											<Button size="sm" variant="ghost" onclick={() => onconvert(item)} disabled={item.heldQty <= 0}>Nanocubes</Button>
										{/if}
										<Button size="sm" variant="ghost" onclick={() => onremove(item)} disabled={item.heldQty <= 0}>Remove</Button>
									</div>
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>
		{/if}
	{:else if visibleEquipment.length === 0}
		<div class="px-6 py-12 text-center">
			<p class="text-sm text-text-tertiary">
				{query ? 'No equipment holding matches that search.' : 'No equipment holdings recorded yet.'}
			</p>
			{#if !query}
				<Button class="mt-4" size="sm" onclick={onaddequipment}>Add your first holding</Button>
			{/if}
		</div>
	{:else}
		<div class="max-h-[34rem] overflow-auto">
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
</Card>
