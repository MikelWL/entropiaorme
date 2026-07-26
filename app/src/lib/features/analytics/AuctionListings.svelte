<script lang="ts">
	/**
	 * The auction worklist and its history.
	 *
	 * Open listings are goods in transit: the stock has left the player's
	 * inventory at a price nobody knows yet. This panel is the only place
	 * that position is visible, and the only place a sale becomes real, so it
	 * leads with what still needs resolving and keeps history beneath it.
	 *
	 * Confirming asks for the price the auction actually fetched and the
	 * additional fee the game charged at the point of sale, because neither
	 * is knowable at listing time.
	 */
	import Button from '$lib/components/Button.svelte';
	import Input from '$lib/components/Input.svelte';
	import InfoTip from '$lib/components/InfoTip.svelte';
	import { NO_DATA, formatPed } from '$lib/utils/format';
	import type { AuctionListing } from '$lib/types/analytics';

	let {
		open,
		resolved,
		onresolve,
	}: {
		open: AuctionListing[];
		resolved: AuctionListing[];
		onresolve: (
			listingId: string,
			outcome:
				| { sold: true; finalPrice: number; saleFee: number; resolvedAt?: string }
				| { sold: false; resolvedAt?: string },
		) => Promise<void>;
	} = $props();

	let confirmingId = $state<string | null>(null);
	let finalPrice = $state(0);
	let saleFee = $state(0);
	let resolvedAt = $state('');
	let busy = $state(false);

	function startConfirm(listing: AuctionListing) {
		confirmingId = listing.id;
		finalPrice = listing.buyout ?? listing.startingBid;
		saleFee = 0;
		resolvedAt = '';
	}

	async function submitSale(listingId: string) {
		if (busy) return;
		busy = true;
		try {
			await onresolve(listingId, {
				sold: true,
				finalPrice,
				saleFee,
				resolvedAt: resolvedAt || undefined,
			});
			confirmingId = null;
		} finally {
			busy = false;
		}
	}

	async function expire(listingId: string) {
		if (busy) return;
		busy = true;
		try {
			await onresolve(listingId, { sold: false });
		} finally {
			busy = false;
		}
	}
</script>

<div class="flex flex-col gap-4">
	<section class="flex flex-col gap-2">
		<div class="flex items-baseline gap-2">
			<h3 class="text-sm font-semibold tracking-tight text-text">On auction</h3>
			<InfoTip align="left" width="w-96" label="About open listings">
				<p class="text-sm text-text-secondary">
					Listed stock has already left your inventory, and its fee is already spent. Nothing here
					counts as a gain yet: an open auction has no price, so there is nothing to realise. Confirm
					it when it sells, or mark it expired when it comes back.
				</p>
			</InfoTip>
		</div>

		{#if open.length === 0}
			<p class="text-sm text-text-tertiary">Nothing on auction.</p>
		{:else}
			<ul class="flex flex-col gap-1">
				{#each open as listing (listing.id)}
					<li class="rounded-md border border-border/50 px-2.5 py-2">
						<div class="flex items-center gap-3">
							<span class="flex-1 min-w-0 text-sm font-medium truncate text-text">
								{listing.itemName}
							</span>
							<span class="w-16 text-right shrink-0 text-sm tabular-nums text-text-secondary">
								{listing.quantity}
							</span>
							<span class="w-24 text-right shrink-0 text-sm tabular-nums text-text-secondary">
								{formatPed(listing.ttValue)} TT
							</span>
							<span class="w-28 text-right shrink-0 text-sm tabular-nums text-text-secondary">
								{formatPed(listing.startingBid)}{listing.buyout !== null
									? ` / ${formatPed(listing.buyout)}`
									: ''}
							</span>
							<span class="w-24 text-right shrink-0 text-xs tabular-nums text-text-tertiary">
								{listing.listedAt}
							</span>
							<div class="shrink-0 flex items-center gap-1.5">
								<Button size="sm" onclick={() => startConfirm(listing)}>Sold</Button>
								<Button size="sm" variant="ghost" onclick={() => expire(listing.id)} disabled={busy}>
									Expired
								</Button>
							</div>
						</div>

						{#if confirmingId === listing.id}
							<div class="mt-2 pt-2 border-t border-border/50 space-y-3">
								<div class="grid grid-cols-3 gap-3">
									<label class="block space-y-1">
										<span class="eyebrow text-text-tertiary">Sold for (PED)</span>
										<Input type="number" min="0" step="0.01" bind:value={finalPrice} />
									</label>
									<label class="block space-y-1">
										<span class="eyebrow text-text-tertiary">Extra fee (PED)</span>
										<Input type="number" min="0" step="0.01" bind:value={saleFee} />
									</label>
									<label class="block space-y-1">
										<span class="eyebrow text-text-tertiary">Sold on</span>
										<Input type="date" bind:value={resolvedAt} />
									</label>
								</div>
								<p class="text-xs text-text-tertiary">
									The extra fee is the one charged at the point of sale when an item clears above
									its starting bid; the game sends it to you by in-game mail. Leave it at zero if
									there was none.
								</p>
								<div class="flex items-center justify-end gap-2">
									<Button variant="ghost" size="sm" onclick={() => (confirmingId = null)}>
										Cancel
									</Button>
									<Button size="sm" onclick={() => submitSale(listing.id)} loading={busy}>
										Confirm sale
									</Button>
								</div>
							</div>
						{/if}
					</li>
				{/each}
			</ul>
		{/if}
	</section>

	<section class="flex flex-col gap-2">
		<h3 class="text-sm font-semibold tracking-tight text-text">Resolved</h3>
		{#if resolved.length === 0}
			<p class="text-sm text-text-tertiary">No resolved listings yet.</p>
		{:else}
			<ul class="flex flex-col gap-1">
				{#each resolved as listing (listing.id)}
					<li class="flex items-center gap-3 rounded-md px-2.5 py-2">
						<span class="flex-1 min-w-0 text-sm truncate text-text-secondary">
							{listing.itemName}
						</span>
						<span class="w-16 text-right shrink-0 text-sm tabular-nums text-text-tertiary">
							{listing.quantity}
						</span>
						<span
							class="w-20 text-right shrink-0 text-xs uppercase tracking-wide
								{listing.status === 'sold' ? 'text-success' : 'text-text-tertiary'}"
						>
							{listing.status}
						</span>
						<span class="w-28 text-right shrink-0 text-sm tabular-nums text-text-secondary">
							{listing.finalPrice !== null ? `${formatPed(listing.finalPrice)} PED` : NO_DATA}
						</span>
						<span
							class="w-28 text-right shrink-0 text-sm tabular-nums font-medium
								{(listing.activityNetMarkup ?? 0) >= 0 ? 'text-success' : 'text-error'}"
						>
							{listing.activityNetMarkup !== null
								? `${formatPed(listing.activityNetMarkup)} MU`
								: NO_DATA}
						</span>
						<span class="w-24 text-right shrink-0 text-xs tabular-nums text-text-tertiary">
							{listing.resolvedAt ?? NO_DATA}
						</span>
					</li>
				{/each}
			</ul>
		{/if}
	</section>
</div>
