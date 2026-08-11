<script lang="ts">
	/**
	 * Sale intake that starts from the game's window rather than from a row.
	 *
	 * The existing sale flow starts by picking a holding, which assumes the
	 * player already knows which tracked position they are selling. In front
	 * of the game's sale window they are reading it off the screen instead, so
	 * this flow runs the other way: record what the window says, then resolve
	 * which holding it refers to, then review before anything is written.
	 *
	 * Resolution is conservative by construction (`inventoryDraftResolve`
	 * returns a winner only for an unambiguous match). An ambiguous name is a
	 * question put to the player, never a quiet choice of cost basis, because
	 * the wrong holding attributes real money to gameplay that did not earn it.
	 *
	 * The capture buttons are the same flow with the fields filled by reading
	 * the screen instead of by typing. They are inert until that lands, and
	 * marked as such: typing remains a complete path, not a fallback.
	 */
	import Button from '$lib/components/Button.svelte';
	import ErrorNotice from '$lib/components/ErrorNotice.svelte';
	import Input from '$lib/components/Input.svelte';
	import Modal from '$lib/components/Modal.svelte';
	import SegmentedControl from '$lib/components/SegmentedControl.svelte';
	import { InDevelopmentMark, inDevelopment } from '$lib/inDevelopment';
	import { formatPed, todayDate } from '$lib/utils/format';
	import type { InventoryHoldingCandidate } from '$lib/api/commands.gen';
	import {
		draftIssues,
		EMPTY_DRAFT,
		impliedMarkupPct,
		isCommittable,
		type ListingDraftFields,
		previewNetMarkup,
	} from './listingIntake';

	let {
		open = $bindable(false),
		onresolve,
		onsubmit,
	}: {
		open?: boolean;
		/** Candidate holdings for a typed or captured name. */
		onresolve: (
			name: string,
			channel: 'auction' | 'trade',
		) => Promise<{ candidates: InventoryHoldingCandidate[]; resolved: InventoryHoldingCandidate | null }>;
		onsubmit: (input: {
			fields: ListingDraftFields;
			channel: 'auction' | 'trade';
			holding: InventoryHoldingCandidate;
			occurredAt: string | null;
		}) => Promise<void>;
	} = $props();

	let fields = $state<ListingDraftFields>({ ...EMPTY_DRAFT });
	let channel = $state<'auction' | 'trade'>('auction');
	let occurredAt = $state(todayDate());
	let saving = $state(false);
	let error = $state<string | null>(null);

	let resolving = $state(false);
	let resolvedFor = $state<string | null>(null);
	let candidates = $state<InventoryHoldingCandidate[]>([]);
	let chosen = $state<InventoryHoldingCandidate | null>(null);

	const issues = $derived(draftIssues(fields, channel));
	const blocking = $derived(issues.filter((issue) => issue.severity === 'blocking'));
	const advisories = $derived(issues.filter((issue) => issue.severity === 'advisory'));
	const netPreview = $derived(previewNetMarkup(fields, channel));
	const impliedSb = $derived(impliedMarkupPct(fields.startingBid, fields.ttValue));
	const impliedBo = $derived(impliedMarkupPct(fields.buyout, fields.ttValue));
	// A name that resolved to nothing is a legitimate sale of untracked stock,
	// so it may proceed; it just cannot claim any activity's provenance.
	const untracked = $derived(resolvedFor !== null && candidates.length === 0);
	const canCommit = $derived(
		isCommittable(fields, channel) && !saving && (chosen !== null || untracked),
	);

	function reset() {
		fields = { ...EMPTY_DRAFT };
		channel = 'auction';
		occurredAt = todayDate();
		error = null;
		resolvedFor = null;
		candidates = [];
		chosen = null;
	}

	$effect(() => {
		if (open) return;
		reset();
	});

	async function resolveName() {
		const name = fields.itemName.trim();
		if (name === '' || resolving) return;
		resolving = true;
		error = null;
		try {
			const outcome = await onresolve(name, channel);
			candidates = outcome.candidates;
			chosen = outcome.resolved;
			resolvedFor = name;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'Could not match that name to a holding';
		} finally {
			resolving = false;
		}
	}

	async function commit() {
		if (!canCommit) return;
		const holding: InventoryHoldingCandidate = chosen ?? {
			kind: 'loot',
			holdingId: fields.itemName.trim(),
			name: fields.itemName.trim(),
			score: 0,
		};
		saving = true;
		error = null;
		try {
			await onsubmit({ fields, channel, holding, occurredAt: occurredAt || null });
			open = false;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'Failed to record the sale';
		} finally {
			saving = false;
		}
	}
</script>

<Modal bind:open title="Create listing" class="max-w-xl">
	<div class="space-y-5">
		{#if inDevelopment.visible}
			<div class="flex flex-wrap items-center gap-2">
				<Button variant="secondary" size="sm" disabled>Capture from game</Button>
				<Button variant="ghost" size="sm" disabled>Capture overlay</Button>
				<InDevelopmentMark id="market-sale-capture" align="left" />
			</div>
		{/if}

		<SegmentedControl
			options={[
				{ id: 'auction', label: 'Auction' },
				{ id: 'trade', label: 'Trade' },
			]}
			active={channel}
			onchange={(id) => (channel = id as 'auction' | 'trade')}
		/>

		<div class="space-y-1">
			<span class="eyebrow text-text-tertiary">Item</span>
			<div class="flex items-start gap-2">
				<Input
					class="flex-1"
					placeholder="As the window names it"
					bind:value={fields.itemName}
					onblur={resolveName}
				/>
				<Button variant="ghost" size="sm" onclick={resolveName} loading={resolving}>Match</Button>
			</div>

			{#if resolving}
				<p class="text-xs text-text-tertiary">Matching against your holdings...</p>
			{:else if chosen}
				<p class="text-xs text-text-secondary">
					Selling from <span class="text-text">{chosen.name}</span>
					{#if chosen.kind === 'equipment'}(asset){/if}
					{#if candidates.length > 1}
						<button
							type="button"
							class="ml-1 text-accent underline-offset-2 hover:underline"
							onclick={() => (chosen = null)}
						>
							change
						</button>
					{/if}
				</p>
			{:else if candidates.length > 0}
				<div class="space-y-1 pt-1">
					<p class="text-xs text-warning">
						More than one holding could be meant. Choose which one this sale takes from.
					</p>
					<div class="flex flex-wrap gap-1.5">
						{#each candidates as candidate (candidate.holdingId)}
							<button
								type="button"
								class="rounded border border-border px-2 py-1 text-xs text-text-secondary
									hover:border-border-bright hover:text-text"
								onclick={() => (chosen = candidate)}
							>
								{candidate.name}
							</button>
						{/each}
					</div>
				</div>
			{:else if untracked}
				<p class="text-xs text-text-tertiary">
					Nothing tracked matches that name. The sale will still be recorded, but no activity can
					claim its markup.
				</p>
			{/if}
		</div>

		<div class="grid grid-cols-2 gap-3">
			<label class="block space-y-1">
				<span class="eyebrow text-text-tertiary">Quantity</span>
				<Input type="number" min="0" step="1" align="right" bind:value={fields.quantity} />
			</label>
			<label class="block space-y-1">
				<span class="eyebrow text-text-tertiary">TT value (PED)</span>
				<Input type="number" min="0" step="0.01" align="right" bind:value={fields.ttValue} />
			</label>
		</div>

		{#if channel === 'auction'}
			<div class="grid grid-cols-2 gap-3">
				<label class="block space-y-1">
					<span class="eyebrow text-text-tertiary">Starting bid (PED)</span>
					<Input type="number" min="0" step="0.01" align="right" bind:value={fields.startingBid} />
				</label>
				<label class="block space-y-1">
					<span class="eyebrow text-text-tertiary">Buyout (PED)</span>
					<Input type="number" min="0" step="0.01" align="right" bind:value={fields.buyout} />
				</label>
			</div>

			<div class="grid grid-cols-2 gap-3">
				<label class="block space-y-1">
					<span class="eyebrow text-text-tertiary">Markup, starting bid (%)</span>
					<Input type="number" min="0" step="0.01" align="right" bind:value={fields.markupSbPct} />
					{#if impliedSb !== null}
						<span class="block text-[10px] text-text-tertiary">
							Your figures imply {impliedSb.toFixed(2)}%
						</span>
					{/if}
				</label>
				<label class="block space-y-1">
					<span class="eyebrow text-text-tertiary">Markup, buyout (%)</span>
					<Input type="number" min="0" step="0.01" align="right" bind:value={fields.markupBoPct} />
					{#if impliedBo !== null}
						<span class="block text-[10px] text-text-tertiary">
							Your figures imply {impliedBo.toFixed(2)}%
						</span>
					{/if}
				</label>
			</div>

			<div class="grid grid-cols-3 gap-3">
				<label class="block space-y-1">
					<span class="eyebrow text-text-tertiary">Auction fee (PED)</span>
					<Input type="number" min="0" step="0.01" align="right" bind:value={fields.auctionFee} />
				</label>
				<label class="block space-y-1">
					<span class="eyebrow text-text-tertiary">Runs for (days)</span>
					<Input type="number" min="1" step="1" align="right" bind:value={fields.auctionDays} />
				</label>
				<label class="block space-y-1">
					<span class="eyebrow text-text-tertiary">Listed on</span>
					<Input type="date" bind:value={occurredAt} />
				</label>
			</div>
		{:else}
			<div class="grid grid-cols-2 gap-3">
				<label class="block space-y-1">
					<span class="eyebrow text-text-tertiary">Sold for (PED)</span>
					<Input type="number" min="0" step="0.01" align="right" bind:value={fields.buyout} />
				</label>
				<label class="block space-y-1">
					<span class="eyebrow text-text-tertiary">Sold on</span>
					<Input type="date" bind:value={occurredAt} />
				</label>
			</div>
		{/if}

		{#if netPreview !== null}
			<div class="flex items-center justify-between border-t border-border/50 pt-3 text-sm">
				<span class="text-text-secondary">
					{channel === 'auction' ? 'Net markup if it clears' : 'Realised markup'}
				</span>
				<span class="tabular-nums font-medium {netPreview >= 0 ? 'text-success' : 'text-error'}">
					{formatPed(netPreview)} PED
				</span>
			</div>
		{/if}

		{#each blocking as issue (issue.field + issue.message)}
			<p class="text-xs text-error">{issue.message}</p>
		{/each}
		{#each advisories as issue (issue.field + issue.message)}
			<p class="text-xs text-warning">{issue.message}</p>
		{/each}

		<ErrorNotice message={error} />

		<div class="flex items-center justify-end gap-2 pt-1">
			<Button variant="ghost" onclick={() => (open = false)} disabled={saving}>Cancel</Button>
			<Button onclick={commit} loading={saving} disabled={!canCommit}>
				{channel === 'auction' ? 'List on auction' : 'Record trade'}
			</Button>
		</div>
	</div>
</Modal>
