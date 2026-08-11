<script lang="ts">
	/**
	 * Sale intake that starts from the game's window rather than from a row.
	 *
	 * The existing sale flow starts by picking a holding, which assumes the
	 * player already knows which tracked position they are selling. In front
	 * of the game's sale window they are reading it off the screen instead, so
	 * this flow runs the other way: record what the window says, resolve which
	 * holding it refers to, then review before anything is written.
	 *
	 * Naming the item is a typeahead over what is actually held, so the usual
	 * case resolves by being chosen rather than by being matched. A name typed
	 * free still resolves on the way out of the field, conservatively: an
	 * ambiguous one is a question put to the player, never a quiet choice of
	 * cost basis, because the wrong holding attributes real money to gameplay
	 * that did not earn it.
	 *
	 * Nothing is complained about until the player tries to file the listing.
	 * A form that opens already scolding the reader for not having filled it
	 * in yet teaches them to ignore its red text.
	 *
	 * The capture buttons are the same flow with the fields filled by reading
	 * the screen instead of by typing. They are inert until that lands, and
	 * marked as such: typing remains a complete path, not a fallback.
	 */
	import { onDestroy } from 'svelte';
	import Button from '$lib/components/Button.svelte';
	import ErrorNotice from '$lib/components/ErrorNotice.svelte';
	import Input from '$lib/components/Input.svelte';
	import Modal from '$lib/components/Modal.svelte';
	import PickerInput from '$lib/components/PickerInput.svelte';
	import SegmentedControl from '$lib/components/SegmentedControl.svelte';
	import { InDevelopmentMark, inDevelopment } from '$lib/inDevelopment';
	import { formatPed } from '$lib/utils/format';
	import { createTypeahead } from '$lib/view/typeahead.svelte';
	import type { InventoryHoldingCandidate } from '$lib/api/commands.gen';
	import {
		draftIssues,
		EMPTY_DRAFT,
		impliedMarkupPct,
		type ListingDraftFields,
		previewNetMarkup,
	} from './listingIntake';

	let {
		open = $bindable(false),
		holdings,
		onresolve,
		onsubmit,
	}: {
		open?: boolean;
		/** Everything currently held, loot and assets alike, for the typeahead. */
		holdings: InventoryHoldingCandidate[];
		/** Candidate holdings for a name typed rather than chosen. */
		onresolve: (
			name: string,
			channel: 'auction' | 'trade',
		) => Promise<{
			candidates: InventoryHoldingCandidate[];
			resolved: InventoryHoldingCandidate | null;
		}>;
		onsubmit: (input: {
			fields: ListingDraftFields;
			channel: 'auction' | 'trade';
			holding: InventoryHoldingCandidate;
			/** A full local timestamp, or null to mean now. */
			occurredAt: string | null;
		}) => Promise<void>;
	} = $props();

	let fields = $state<ListingDraftFields>({ ...EMPTY_DRAFT });
	let channel = $state<'auction' | 'trade'>('auction');
	// Null means now, resolved at the moment of filing rather than pinned when
	// the form opened: a form left sitting for ten minutes should still record
	// the listing as made when it was actually made.
	let occurredAt = $state<string | null>(null);
	let saving = $state(false);
	let error = $state<string | null>(null);
	let attempted = $state(false);

	let resolving = $state(false);
	let candidates = $state<InventoryHoldingCandidate[]>([]);
	let chosen = $state<InventoryHoldingCandidate | null>(null);

	const picker = createTypeahead<InventoryHoldingCandidate>({
		search: async (query) => {
			const needle = query.trim().toLowerCase();
			return holdings.filter((row) => row.name.toLowerCase().includes(needle)).slice(0, 12);
		},
		debounceMs: 0,
		minLength: 1,
		labelOf: (row) => row.name,
	});

	const pickerModel = {
		get query() {
			return picker.query;
		},
		set query(value: string) {
			picker.query = value;
			fields.itemName = value;
			// The match belongs to the name it was made for; editing the name
			// must drop it, or a sale could bind to the holding of a word the
			// player has since typed over.
			chosen = null;
			candidates = [];
		},
		get results() {
			return picker.results;
		},
		get selected() {
			return picker.selected;
		},
		get loading() {
			return picker.loading;
		},
		get error() {
			return picker.error;
		},
		select(row: InventoryHoldingCandidate) {
			picker.select(row);
			fields.itemName = row.name;
			chosen = row;
			candidates = [];
		},
		clear() {
			// `clear` empties the picker's own query directly, so the draft's
			// copy of the name has to be emptied with it or the form would
			// still be holding a name the box no longer shows.
			picker.clear();
			fields.itemName = '';
			chosen = null;
			candidates = [];
		},
	};

	const issues = $derived(draftIssues(fields, channel));
	const blocking = $derived(issues.filter((issue) => issue.severity === 'blocking'));
	const advisories = $derived(issues.filter((issue) => issue.severity === 'advisory'));
	const netPreview = $derived(previewNetMarkup(fields, channel));
	const impliedSb = $derived(impliedMarkupPct(fields.startingBid, fields.ttValue));
	const impliedBo = $derived(impliedMarkupPct(fields.buyout, fields.ttValue));
	const named = $derived(fields.itemName.trim() !== '');
	// A name that resolved to nothing is a legitimate sale of untracked stock,
	// so it may proceed; it just cannot claim any activity's provenance.
	const untracked = $derived(named && !chosen && candidates.length === 0 && !resolving);

	function reset() {
		fields = { ...EMPTY_DRAFT };
		channel = 'auction';
		occurredAt = null;
		error = null;
		attempted = false;
		candidates = [];
		chosen = null;
		picker.clear();
	}

	$effect(() => {
		if (open) return;
		reset();
	});

	onDestroy(() => picker.destroy());

	/** Resolve a name the player typed instead of choosing. */
	async function resolveTyped() {
		const name = fields.itemName.trim();
		if (name === '' || chosen || resolving) return;
		resolving = true;
		try {
			const outcome = await onresolve(name, channel);
			candidates = outcome.candidates;
			chosen = outcome.resolved;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'Could not match that name to a holding';
		} finally {
			resolving = false;
		}
	}

	async function commit() {
		attempted = true;
		if (saving || blocking.length > 0) return;
		await resolveTyped();
		// An unresolved ambiguity is the one thing the player must settle;
		// everything else the form already knows how to proceed without.
		if (!chosen && candidates.length > 0) return;
		const holding: InventoryHoldingCandidate = chosen ?? {
			kind: 'loot',
			holdingId: fields.itemName.trim(),
			name: fields.itemName.trim(),
			score: 0,
		};
		saving = true;
		error = null;
		try {
			await onsubmit({ fields, channel, holding, occurredAt });
			open = false;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'Failed to record the sale';
		} finally {
			saving = false;
		}
	}

	/** A local `datetime-local` value for now, for the moment the player
	 * chooses to pin the time rather than leave it running. */
	function nowLocalValue(): string {
		const now = new Date();
		const pad = (value: number) => String(value).padStart(2, '0');
		return (
			`${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}` +
			`T${pad(now.getHours())}:${pad(now.getMinutes())}`
		);
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
			<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
			<div onfocusout={resolveTyped}>
				<PickerInput
					id="create-listing-item"
					model={pickerModel}
					class="relative"
					dropdownClass="absolute left-0 right-0 z-30 shadow-lg"
				>
					{#snippet result({ item })}
						<span class="truncate">{item.name}</span>
						{#if item.kind === 'equipment'}
							<span class="ml-3 shrink-0 text-xs text-text-tertiary">Asset</span>
						{/if}
					{/snippet}
					{#snippet selection({ item })}
						<span class="truncate">{item.name}</span>
					{/snippet}
				</PickerInput>
			</div>

			{#if !chosen && candidates.length > 0}
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
			<!-- Markup is what the bids come to against TT, exactly as the game
				shows it: read-only there, so read-only here. -->
			<div class="grid grid-cols-2 gap-3">
				<label class="block space-y-1">
					<span class="eyebrow text-text-tertiary">Starting bid (PED)</span>
					<Input type="number" min="0" step="0.01" align="right" bind:value={fields.startingBid} />
					<span class="block text-right text-[10px] tabular-nums text-text-tertiary">
						{impliedSb !== null ? `${impliedSb.toFixed(2)}% of TT` : ' '}
					</span>
				</label>
				<label class="block space-y-1">
					<span class="eyebrow text-text-tertiary">Buyout (PED)</span>
					<Input type="number" min="0" step="0.01" align="right" bind:value={fields.buyout} />
					<span class="block text-right text-[10px] tabular-nums text-text-tertiary">
						{impliedBo !== null ? `${impliedBo.toFixed(2)}% of TT` : ' '}
					</span>
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
				<div class="block space-y-1">
					<span class="eyebrow text-text-tertiary">Listed</span>
					{#if occurredAt === null}
						<button
							type="button"
							class="h-9 w-full rounded-md border border-border bg-surface/70 px-3 text-left text-sm
								text-text transition-[border-color] hover:border-border-bright"
							onclick={() => (occurredAt = nowLocalValue())}
						>
							Now
						</button>
					{:else}
						<div class="flex items-center gap-1">
							<Input type="datetime-local" class="flex-1" bind:value={occurredAt} />
							<button
								type="button"
								class="shrink-0 px-1 text-xs text-accent underline-offset-2 hover:underline"
								onclick={() => (occurredAt = null)}
							>
								Now
							</button>
						</div>
					{/if}
				</div>
			</div>
		{:else}
			<div class="grid grid-cols-2 gap-3">
				<label class="block space-y-1">
					<span class="eyebrow text-text-tertiary">Sold for (PED)</span>
					<Input type="number" min="0" step="0.01" align="right" bind:value={fields.buyout} />
				</label>
				<div class="block space-y-1">
					<span class="eyebrow text-text-tertiary">Sold</span>
					{#if occurredAt === null}
						<button
							type="button"
							class="h-9 w-full rounded-md border border-border bg-surface/70 px-3 text-left text-sm
								text-text transition-[border-color] hover:border-border-bright"
							onclick={() => (occurredAt = nowLocalValue())}
						>
							Now
						</button>
					{:else}
						<div class="flex items-center gap-1">
							<Input type="datetime-local" class="flex-1" bind:value={occurredAt} />
							<button
								type="button"
								class="shrink-0 px-1 text-xs text-accent underline-offset-2 hover:underline"
								onclick={() => (occurredAt = null)}
							>
								Now
							</button>
						</div>
					{/if}
				</div>
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

		{#if attempted}
			{#each blocking as issue (issue.field + issue.message)}
				<p class="text-xs text-error">{issue.message}</p>
			{/each}
		{/if}
		{#each advisories as issue (issue.field + issue.message)}
			<p class="text-xs text-warning">{issue.message}</p>
		{/each}

		<ErrorNotice message={error} />

		<div class="flex items-center justify-end gap-2 pt-1">
			<Button variant="ghost" onclick={() => (open = false)} disabled={saving}>Cancel</Button>
			<Button onclick={commit} loading={saving}>
				{channel === 'auction' ? 'List on auction' : 'Record trade'}
			</Button>
		</div>
	</div>
</Modal>
