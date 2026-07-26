<script lang="ts">
	/**
	 * Recycle held stock into Nanocubes.
	 *
	 * A transformation, not a sale: TT is preserved exactly (100 PED of wood
	 * becomes 100 PED of Nanocubes, with no refiner cost), no markup is
	 * realised, and the ledger is untouched. The consumed stock's activity
	 * composition rides forward into the Nanocubes, so selling them later
	 * still attributes back to the tiers that grew the wood.
	 */
	import Modal from '$lib/components/Modal.svelte';
	import Button from '$lib/components/Button.svelte';
	import Input from '$lib/components/Input.svelte';
	import { formatPed } from '$lib/utils/format';
	import { NANOCUBE_ITEM, type TreeCuttingStock } from './treeCuttingModel.svelte';

	let {
		item,
		onconvert,
		oncancel,
	}: {
		item: TreeCuttingStock | null;
		onconvert: (sourceItem: string, quantity: number) => Promise<void>;
		oncancel: () => void;
	} = $props();

	let quantity = $state(0);
	let converting = $state(false);
	let error = $state<string | null>(null);

	let modalOpen = $state(false);
	let initialisedFor = $state<string | null>(null);

	const unitTt = $derived(item && item.heldQty > 0 ? item.heldTt / item.heldQty : 0);
	const convertedTt = $derived(quantity * unitTt);

	$effect(() => {
		if (item && initialisedFor !== item.itemName) {
			quantity = item.heldQty;
			error = null;
			initialisedFor = item.itemName;
			modalOpen = true;
		}
		if (!item) {
			initialisedFor = null;
			modalOpen = false;
		}
	});

	$effect(() => {
		if (item && !modalOpen) oncancel();
	});

	async function confirm() {
		if (!item || converting || quantity <= 0) return;
		converting = true;
		error = null;
		try {
			await onconvert(item.itemName, quantity);
			modalOpen = false;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to convert the stock';
		} finally {
			converting = false;
		}
	}
</script>

{#if item}
	<Modal bind:open={modalOpen} title={`Convert ${item.itemName}`}>
		<div class="space-y-4">
			<div class="bg-surface/50 rounded-md border border-border/50 px-3 py-2 space-y-1.5 text-sm">
				<div class="flex items-center justify-between">
					<span class="text-text-secondary">Held</span>
					<span class="tabular-nums text-text">{item.heldQty} ({formatPed(item.heldTt)} PED)</span>
				</div>
				<div class="flex items-center justify-between pt-1.5 border-t border-border/50">
					<span class="text-text font-medium">{NANOCUBE_ITEM} produced</span>
					<span class="tabular-nums text-text font-medium">{formatPed(convertedTt)} PED</span>
				</div>
			</div>

			<label class="block space-y-1">
				<span class="eyebrow text-text-tertiary">Quantity to convert</span>
				<Input type="number" min="0" step="1" bind:value={quantity} />
			</label>

			<p class="text-xs text-text-tertiary">
				TT is preserved exactly. This records no gain or loss and writes nothing to your ledger;
				the resulting {NANOCUBE_ITEM}s keep the source activity behind them, so selling them still
				credits the tiers that produced the wood.
			</p>

			{#if error}
				<p class="text-xs text-error">{error}</p>
			{/if}

			<div class="flex items-center justify-end gap-2 pt-2">
				<Button variant="ghost" onclick={oncancel} disabled={converting}>Cancel</Button>
				<Button onclick={confirm} loading={converting} disabled={quantity <= 0}>Convert</Button>
			</div>
		</div>
	</Modal>
{/if}
