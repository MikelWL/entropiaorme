<script lang="ts">
	import type { InventoryItem, InventorySellResult } from '$lib/types/analytics';
	import { sellInventoryItem } from '$lib/api';
	import { formatPed } from '$lib/utils/format';
	import Modal from '$lib/components/Modal.svelte';
	import Button from '$lib/components/Button.svelte';
	import Input from '$lib/components/Input.svelte';

	let {
		item,
		prefilledSalePrice = null,
		onsold,
		oncancel,
	}: {
		item: InventoryItem | null;
		prefilledSalePrice?: number | null;
		onsold: (result: InventorySellResult) => void;
		oncancel: () => void;
	} = $props();

	let salePrice = $state(0);
	let description = $state('');
	let descriptionTouched = $state(false);
	let selling = $state(false);
	let error = $state<string | null>(null);

	let costBasis = $derived(item ? item.ttValue + item.markupPaid : 0);
	let delta = $derived(salePrice - costBasis);
	let autoDescription = $derived(item ? `Inventory Sale: ${item.name}` : '');

	let modalOpen = $state(false);
	let initialisedFor = $state<string | null>(null);

	$effect(() => {
		if (item && initialisedFor !== item.id) {
			salePrice = prefilledSalePrice ?? 0;
			description = `Inventory Sale: ${item.name}`;
			descriptionTouched = false;
			error = null;
			initialisedFor = item.id;
			modalOpen = true;
		}
		if (!item) {
			initialisedFor = null;
			modalOpen = false;
		}
	});

	$effect(() => {
		if (item && !modalOpen) {
			oncancel();
		}
	});

	async function confirm() {
		if (!item || selling) return;
		selling = true;
		error = null;
		try {
			const payload: { sale_price: number; description?: string } = {
				sale_price: salePrice,
			};
			const finalDescription = description.trim();
			if (descriptionTouched && finalDescription && finalDescription !== autoDescription) {
				payload.description = finalDescription;
			}
			const result = await sellInventoryItem(item.id, payload);
			onsold(result);
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to sell inventory item';
		} finally {
			selling = false;
		}
	}
</script>

{#if item}
	<Modal bind:open={modalOpen} title={`Sell ${item.name}`}>
		<div class="space-y-4">
			<div class="bg-surface/50 rounded-md border border-border/50 px-3 py-2 space-y-1.5 text-sm">
				<div class="flex items-center justify-between">
					<span class="text-text-secondary">Original TT</span>
					<span class="tabular-nums text-text">{formatPed(item.ttValue)} PED</span>
				</div>
				<div class="flex items-center justify-between">
					<span class="text-text-secondary">Markup paid</span>
					<span class="tabular-nums text-text">{formatPed(item.markupPaid)} PED</span>
				</div>
				<div class="flex items-center justify-between pt-1.5 border-t border-border/50">
					<span class="text-text font-medium">Cost basis</span>
					<span class="tabular-nums text-text font-semibold">{formatPed(costBasis)} PED</span>
				</div>
			</div>

			<div>
				<label for="sale-price" class="text-xs text-text-secondary mb-1 block">Sale price (PED)</label>
				<!-- svelte-ignore a11y_autofocus -->
				<Input
					id="sale-price"
					type="number"
					bind:value={salePrice}
					step="0.01"
					min="0"
					placeholder="0.00"
					autofocus
				/>
			</div>

			{#if delta !== 0}
				<div
					class="rounded-md px-3 py-2 flex items-center justify-between text-sm
						{delta > 0 ? 'bg-positive/10 border border-positive/30' : 'bg-negative/10 border border-negative/30'}"
				>
					<span class="{delta > 0 ? 'text-positive' : 'text-negative'} font-medium">
						{delta > 0 ? 'Gain' : 'Loss'}
					</span>
					<span
						class="tabular-nums font-semibold {delta > 0 ? 'text-positive' : 'text-negative'}"
					>
						{delta > 0 ? '+' : '-'}{formatPed(Math.abs(delta))} PED
					</span>
				</div>

				<div class="bg-surface/50 rounded-md border border-border/50 px-3 py-3 space-y-2">
					<span class="text-xs text-text-secondary uppercase tracking-wide">
						Ledger entry
					</span>
					<div class="space-y-1.5 text-sm">
						<div class="flex items-center justify-between">
							<span class="text-text-secondary">Type</span>
							<span class="{delta > 0 ? 'text-positive' : 'text-negative'} font-medium">
								{delta > 0 ? 'Markup (+)' : 'Expense (−)'}
							</span>
						</div>
						<div class="flex items-center justify-between">
							<span class="text-text-secondary">Amount</span>
							<span class="tabular-nums text-text font-medium">
								{formatPed(Math.abs(delta))} PED
							</span>
						</div>
						<div class="flex items-center justify-between">
							<span class="text-text-secondary">Tag</span>
							<span class="text-text">Inventory Sale</span>
						</div>
						<div>
							<label for="sale-desc" class="text-text-secondary block mb-1">Description</label>
							<Input
								id="sale-desc"
								type="text"
								bind:value={description}
								oninput={() => { descriptionTouched = true; }}
							/>
						</div>
					</div>
				</div>
			{:else}
				<div class="bg-surface/50 rounded-md border border-border/50 px-3 py-3 text-sm text-text-secondary">
					Sale matches cost basis: no Ledger entry will be emitted.
				</div>
			{/if}

			{#if error}
				<p class="text-xs text-error">{error}</p>
			{/if}

			<div class="flex items-center justify-end gap-2 pt-2">
				<Button variant="ghost" onclick={oncancel} disabled={selling}>Cancel</Button>
				<Button onclick={confirm} loading={selling}>Confirm Sale</Button>
			</div>
		</div>
	</Modal>
{/if}
