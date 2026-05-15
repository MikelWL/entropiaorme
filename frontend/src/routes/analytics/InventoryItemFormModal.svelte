<script lang="ts">
	import type { InventoryItem } from '$lib/types/analytics';
	import { addInventoryItem, updateInventoryItem } from '$lib/api';
	import { formatPed } from '$lib/utils/format';
	import Modal from '$lib/components/Modal.svelte';
	import Button from '$lib/components/Button.svelte';
	import Input from '$lib/components/Input.svelte';
	import SegmentedControl from '$lib/components/SegmentedControl.svelte';

	let {
		open = $bindable(false),
		item = null,
		onsaved,
	}: {
		open?: boolean;
		item?: InventoryItem | null;
		onsaved: (item: InventoryItem) => void;
	} = $props();

	type MarkupMode = 'percent' | 'ped';

	let name = $state('');
	let ttValue = $state(0);
	let markupMode = $state<MarkupMode>('percent');
	let markupPed = $state(0);
	let markupPercent = $state(100);
	let notes = $state('');
	let saving = $state(false);
	let error = $state<string | null>(null);

	let initialised = $state(false);

	$effect(() => {
		if (open && !initialised) {
			if (item) {
				name = item.name;
				ttValue = item.ttValue;
				markupPed = item.markupPaid;
				markupPercent = item.ttValue > 0 ? 100 + (item.markupPaid / item.ttValue) * 100 : 100;
				notes = item.notes ?? '';
			} else {
				name = '';
				ttValue = 0;
				markupPed = 0;
				markupPercent = 100;
				notes = '';
			}
			markupMode = 'percent';
			error = null;
			initialised = true;
		}
		if (!open) {
			initialised = false;
		}
	});

	$effect(() => {
		if (markupMode === 'percent' && ttValue > 0) {
			markupPed = Math.round(ttValue * (markupPercent - 100)) / 100;
		}
	});

	$effect(() => {
		if (markupMode === 'ped' && ttValue > 0) {
			markupPercent = Math.round((100 + (markupPed / ttValue) * 100) * 100) / 100;
		}
	});

	let costBasis = $derived(ttValue + markupPed);
	let canSave = $derived(name.trim().length > 0 && ttValue > 0);

	async function save() {
		if (!canSave || saving) return;
		saving = true;
		error = null;
		try {
			const payload = {
				name: name.trim(),
				tt_value: ttValue,
				markup_paid: markupPed,
				notes: notes.trim() || null,
			};
			const saved = item
				? await updateInventoryItem(item.id, payload)
				: await addInventoryItem(payload);
			onsaved(saved);
			open = false;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to save inventory item';
		} finally {
			saving = false;
		}
	}
</script>

<Modal bind:open title={item ? 'Edit Inventory Item' : 'Add Inventory Item'}>
	<div class="space-y-4">
		<div>
			<label for="inventory-name" class="text-xs text-text-secondary mb-1 block">Name</label>
			<Input
				id="inventory-name"
				type="text"
				bind:value={name}
				placeholder="e.g. Mod FAP, Treasure Island deed"
			/>
		</div>

		<div>
			<label for="inventory-tt" class="text-xs text-text-secondary mb-1 block">TT value (PED)</label>
			<Input
				id="inventory-tt"
				type="number"
				bind:value={ttValue}
				step="0.01"
				min="0"
				placeholder="0.00"
			/>
		</div>

		<div>
			<div class="flex items-center justify-between mb-1">
				<span class="text-xs text-text-secondary">Markup</span>
				<SegmentedControl
					options={[
						{ id: 'percent', label: '%', disabled: ttValue <= 0 },
						{ id: 'ped', label: 'PED' }
					]}
					active={markupMode}
					onchange={(id) => (markupMode = id as MarkupMode)}
				/>
			</div>

			{#if markupMode === 'percent'}
				<div class="grid grid-cols-2 gap-3">
					<Input
						type="number"
						bind:value={markupPercent}
						step="0.1"
						min="100"
						placeholder="100"
						disabled={ttValue <= 0}
					/>
					<div
						class="h-9 px-3 flex items-center justify-end text-sm bg-surface/50
							text-text-secondary rounded-md border border-border/50 tabular-nums"
					>
						{ttValue > 0 ? `${formatPed(markupPed)} PED` : '—'}
					</div>
				</div>
			{:else}
				<div class="grid grid-cols-2 gap-3">
					<Input
						type="number"
						bind:value={markupPed}
						step="0.01"
						min="0"
						placeholder="0.00"
					/>
					<div
						class="h-9 px-3 flex items-center justify-end text-sm bg-surface/50
							text-text-secondary rounded-md border border-border/50 tabular-nums"
					>
						{ttValue > 0 ? `${markupPercent.toFixed(2)}%` : '—'}
					</div>
				</div>
			{/if}
		</div>

		<div>
			<label for="inventory-notes" class="text-xs text-text-secondary mb-1 block">
				Notes <span class="text-text-tertiary">(optional)</span>
			</label>
			<textarea
				id="inventory-notes"
				bind:value={notes}
				rows="2"
				placeholder="Anything worth remembering"
				class="w-full px-3 py-2 text-sm bg-surface text-text rounded-md border border-border
					placeholder:text-text-tertiary resize-none
					focus:outline-none focus:border-accent focus:ring-1 focus:ring-accent"
			></textarea>
		</div>

		<div class="bg-surface/50 rounded-md border border-border/50 px-3 py-2 flex items-center justify-between">
			<span class="text-xs text-text-secondary uppercase tracking-wide">Cost basis</span>
			<span class="text-sm font-semibold tabular-nums text-text">
				{formatPed(costBasis)} PED
			</span>
		</div>

		{#if error}
			<p class="text-xs text-error">{error}</p>
		{/if}

		<div class="flex items-center justify-end gap-2 pt-2">
			<Button variant="ghost" onclick={() => (open = false)} disabled={saving}>Cancel</Button>
			<Button onclick={save} loading={saving} disabled={!canSave}>
				{item ? 'Save' : 'Add'}
			</Button>
		</div>
	</div>
</Modal>
