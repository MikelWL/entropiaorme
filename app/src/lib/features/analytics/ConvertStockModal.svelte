<script lang="ts">
	/**
	 * Recycle held stock into Nanocubes.
	 *
	 * A transformation, not a sale: TT is preserved exactly (100 PED of wood
	 * becomes 100 PED of Nanocubes, with no refiner cost), no markup is
	 * realised, and the ledger is untouched. The consumed stock's activity
	 * composition rides forward into the Nanocubes, so selling them later
	 * still attributes back to the tiers that grew the wood.
	 *
	 * Because the conversion is 1:1 there is only ever one decision here: how
	 * much. The modal is built around that single field, with the position it
	 * is drawn from sitting under it as both the context for the number and the
	 * way back to all of it.
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
	const atMax = $derived(!!item && Math.abs(ped - item.heldTt) < 0.005);
	const overStock = $derived(!!item && ped > item.heldTt);

	// A sale may run past tracked stock, because the player can hold units the
	// app never recorded. A conversion may not: what it produces is credited to
	// the activities that grew the source, so converting more than is tracked
	// would credit them with Nanocubes they did not produce. The cap is held
	// here as the value is entered, and again on the action below.
	$effect(() => {
		if (item && ped > item.heldTt) ped = item.heldTt;
	});

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
		if (!item || converting || ped <= 0 || unitTt <= 0 || ped > item.heldTt) return;
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
	<Modal bind:open={modalOpen} class="max-w-xs" title={`Convert ${item.itemName}`}>
		<div class="space-y-5">
			<!-- The amount carries no label of its own. The title says what is
				being converted, the field says PED, and the line beneath says how
				much there is; a caption above it would only say it a fourth time. -->
			<div class="space-y-1.5">
				<Input
					type="number"
					min="0"
					max={item.heldTt}
					step="0.01"
					align="right"
					aria-label="PED to convert"
					bind:value={ped}
				>
					{#snippet suffix()}
						<span class="text-xs font-medium uppercase tracking-wider">PED</span>
					{/snippet}
				</Input>

				<div class="flex items-baseline justify-between gap-2 text-xs">
					<span class="text-text-tertiary tabular-nums">
						{formatPed(item.heldTt)} PED in stock
					</span>
					<button
						type="button"
						disabled={atMax}
						onclick={() => (ped = item.heldTt)}
						class="font-medium text-accent cursor-pointer
							transition-colors duration-[var(--duration-fast)] hover:text-text
							disabled:cursor-default disabled:text-text-tertiary/50 disabled:hover:text-text-tertiary/50"
					>
						All of it
					</button>
				</div>
			</div>

			{#if error}
				<p class="text-xs text-error">{error}</p>
			{/if}

			<div class="flex items-center justify-end gap-2">
				<Button variant="ghost" onclick={oncancel} disabled={converting}>Cancel</Button>
				<Button
					onclick={confirm}
					loading={converting}
					disabled={ped <= 0 || unitTt <= 0 || overStock}
				>
					Convert
				</Button>
			</div>
		</div>
	</Modal>
{/if}
