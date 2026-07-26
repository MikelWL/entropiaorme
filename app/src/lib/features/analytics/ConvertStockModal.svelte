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
	import type { TreeCuttingStock } from './treeCuttingModel.svelte';

	let {
		item,
		onconvert,
		oncancel,
	}: {
		item: TreeCuttingStock | null;
		onconvert: (sourceItem: string, quantity: number) => Promise<void>;
		oncancel: () => void;
	} = $props();

	// The player holds and thinks in PED, and the conversion is 1:1 in TT, so
	// PED is the only figure the modal needs. Units are what the command takes,
	// so the entered PED is divided back through the item's unit TT at the last
	// moment; going the other way would make the player do that arithmetic.
	let ped = $state(0);
	let converting = $state(false);
	let error = $state<string | null>(null);

	let modalOpen = $state(false);
	let initialisedFor = $state<string | null>(null);

	const unitTt = $derived(item && item.heldQty > 0 ? item.heldTt / item.heldQty : 0);

	$effect(() => {
		if (item && initialisedFor !== item.itemName) {
			ped = item.heldTt;
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
		if (!item || converting || ped <= 0 || unitTt <= 0) return;
		converting = true;
		error = null;
		try {
			await onconvert(item.itemName, ped / unitTt);
			modalOpen = false;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to convert the stock';
		} finally {
			converting = false;
		}
	}
</script>

{#if item}
	<!-- Two figures and a button want none of the default panel's width. -->
	<Modal bind:open={modalOpen} class="max-w-xs" title={`Convert ${item.itemName}`}>
		<div class="space-y-4">
			<div class="bg-surface/50 rounded-md border border-border/50 px-3 py-2 text-sm">
				<div class="flex items-center justify-between">
					<span class="text-text-secondary">Held</span>
					<span class="tabular-nums text-text">{formatPed(item.heldTt)} PED</span>
				</div>
			</div>

			<!-- Label left, field right, on the same insets as the row above: the
				amount being entered reads against the amount available. -->
			<label class="flex items-center justify-between gap-3 px-3">
				<span class="eyebrow text-text-tertiary">PED to convert</span>
				<Input class="w-28 shrink-0" type="number" min="0" step="0.01" bind:value={ped} />
			</label>

			{#if error}
				<p class="text-xs text-error">{error}</p>
			{/if}

			<div class="flex items-center justify-end gap-2 pt-2">
				<Button variant="ghost" onclick={oncancel} disabled={converting}>Cancel</Button>
				<Button onclick={confirm} loading={converting} disabled={ped <= 0 || unitTt <= 0}>
					Convert
				</Button>
			</div>
		</div>
	</Modal>
{/if}
