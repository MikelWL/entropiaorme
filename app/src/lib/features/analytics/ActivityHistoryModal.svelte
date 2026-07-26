<script lang="ts">
	/**
	 * What this activity has done to its stock, and the way back out of it.
	 *
	 * A review surface: you open it because something is wrong, so it is built
	 * for finding the entry you mean and understanding it at a glance. Each row
	 * says what happened in a line, carries the one figure that matters on the
	 * right, and keeps its undo quiet until asked.
	 *
	 * A listing appears once however far it got. Creating it and selling it are
	 * the same listing at two moments, so confirming a sale changes the entry
	 * rather than adding another beneath it.
	 *
	 * Undo confirms in place rather than in a second modal. A sold listing has
	 * two ways back and they differ in a way worth reading, which a stacked
	 * dialog would ask you to hold in your head with the row out of sight.
	 */
	import Button from '$lib/components/Button.svelte';
	import InfoTip from '$lib/components/InfoTip.svelte';
	import Modal from '$lib/components/Modal.svelte';
	import type { ActivityHistoryEntry } from '$lib/types/analytics';
	import { formatLedgerDate, formatPed } from '$lib/utils/format';

	let {
		open = $bindable(false),
		entries,
		loading,
		onundo,
	}: {
		open?: boolean;
		entries: ActivityHistoryEntry[];
		loading: boolean;
		onundo: (entry: ActivityHistoryEntry, revertSale: boolean) => Promise<void>;
	} = $props();

	// Which row is asking to be confirmed, and which row is mid-undo. Only one
	// of each: a confirmation is a question about one entry.
	let confirming = $state<string | null>(null);
	let busy = $state<string | null>(null);

	const signedPed = (value: number) => `${value >= 0 ? '+' : ''}${formatPed(value)}`;

	const STATUS_LABEL: Record<string, string> = {
		pending: 'On auction',
		sold: 'Sold',
		expired: 'Expired',
		converted: 'Converted',
	};

	const STATUS_TONE: Record<string, string> = {
		pending: 'text-accent',
		sold: 'text-positive',
		expired: 'text-text-tertiary',
		converted: 'text-text-secondary',
	};

	/** What happened, in a line. The figures a row leads with are on the
	 * right, so this says what was done to how much. */
	function summary(entry: ActivityHistoryEntry): string {
		const tt = `${formatPed(entry.ttValue)} PED TT`;
		if (entry.kind === 'conversion') return `${tt} into ${entry.targetItem ?? 'another item'}`;
		if (entry.status === 'sold') return `${formatPed(entry.quantity)} sold, ${tt}`;
		if (entry.status === 'expired') return `${formatPed(entry.quantity)} returned, ${tt}`;
		return `${formatPed(entry.quantity)} on auction, ${tt}`;
	}

	function startConfirm(entry: ActivityHistoryEntry) {
		confirming = confirming === entry.id ? null : entry.id;
	}

	async function undo(entry: ActivityHistoryEntry, revertSale: boolean) {
		if (busy) return;
		busy = entry.id;
		try {
			await onundo(entry, revertSale);
			confirming = null;
		} catch {
			// The model surfaces the reason on the tab; the row stays open so
			// the choice that failed is still in front of the reader.
		} finally {
			busy = null;
		}
	}

	// Closing puts away any half-asked question with it.
	$effect(() => {
		if (!open) confirming = null;
	});
</script>

{#snippet undoChoice(
	label: string,
	description: string,
	entry: ActivityHistoryEntry,
	revertSale: boolean,
)}
	<button
		type="button"
		disabled={busy !== null}
		onclick={() => undo(entry, revertSale)}
		class="flex-1 rounded-md border border-border/50 bg-surface/40 px-3 py-2 text-left
			transition-[background-color,border-color] duration-[var(--duration-base)] ease-[var(--ease-out)]
			hover:border-border-bright hover:bg-surface-hover/50
			disabled:cursor-not-allowed disabled:opacity-50"
	>
		<span class="block text-xs font-medium text-text">{label}</span>
		<span class="mt-0.5 block text-[0.6875rem] leading-snug text-text-tertiary">
			{description}
		</span>
	</button>
{/snippet}

<Modal bind:open class="max-w-2xl" title="History">
	{#if loading}
		<p class="py-6 text-center text-sm text-text-tertiary">Reading what has been recorded...</p>
	{:else if entries.length === 0}
		<div class="flex min-h-32 items-center justify-center">
			<p class="max-w-sm text-center text-sm leading-relaxed text-text-tertiary">
				Nothing recorded yet. Auction listings and Nanocube conversions appear here, and can be
				taken back from here if you record one by mistake.
			</p>
		</div>
	{:else}
		<ul class="-mx-2 flex max-h-[26rem] flex-col overflow-y-auto px-2">
			{#each entries as entry (entry.id)}
				{@const isConfirming = confirming === entry.id}
				<li
					class="border-b border-border/30 py-3 last:border-b-0
						transition-colors duration-[var(--duration-base)]
						{isConfirming ? 'bg-surface-hover/30' : ''}"
				>
					<div class="flex items-center gap-3">
						<span
							class="w-20 shrink-0 text-[0.625rem] font-medium uppercase tracking-wide
								{STATUS_TONE[entry.status] ?? 'text-text-tertiary'}"
						>
							{STATUS_LABEL[entry.status] ?? entry.status}
						</span>

						<div class="min-w-0 flex-1">
							<p class="truncate text-sm font-medium tracking-tight text-text">
								{entry.itemName}
							</p>
							<p class="mt-0.5 truncate text-xs text-text-tertiary">{summary(entry)}</p>
						</div>

						<div class="w-24 shrink-0 text-right">
							{#if entry.netMarkup !== null}
								<p class="text-sm font-medium tabular-nums text-text">
									{signedPed(entry.netMarkup)}
								</p>
								<p class="mt-0.5 text-[0.625rem] uppercase tracking-wider text-text-tertiary">
									Net markup
								</p>
							{:else}
								<p class="text-sm tabular-nums text-text-tertiary">
									{formatPed(entry.ttValue)}
								</p>
								<p class="mt-0.5 text-[0.625rem] uppercase tracking-wider text-text-tertiary">
									PED
								</p>
							{/if}
						</div>

						<span class="w-14 shrink-0 text-right text-xs tabular-nums text-text-tertiary">
							{formatLedgerDate(entry.occurredAt)}
						</span>

						<div class="flex w-20 shrink-0 items-center justify-end">
							{#if !entry.canDelete && !entry.canRevertSale}
								<span class="flex items-center gap-1 text-xs text-text-tertiary">
									Held
									<InfoTip align="right" label="Why this cannot be undone" width="w-80">
										<p class="text-xs font-semibold leading-relaxed text-text">
											Something else depends on this
										</p>
										<p class="mt-1 text-xs leading-relaxed text-text-secondary">
											{entry.undoBlockedReason}
										</p>
									</InfoTip>
								</span>
							{:else}
								<Button
									variant="ghost"
									size="sm"
									loading={busy === entry.id}
									onclick={() => startConfirm(entry)}
								>
									{isConfirming ? 'Keep' : 'Undo'}
								</Button>
							{/if}
						</div>
					</div>

					{#if isConfirming}
						<!-- The choice sits under the row it belongs to, so what is
							about to be undone stays in front of the reader. -->
						<div class="mt-3 flex gap-2 pl-[5.75rem]">
							{#if entry.canRevertSale}
								{@render undoChoice(
									'Undo the sale',
									'The listing goes back on auction. Its stock stays out and the markup stops being realised.',
									entry,
									true,
								)}
							{/if}
							{#if entry.canDelete}
								{@render undoChoice(
									entry.kind === 'conversion'
										? 'Undo the conversion'
										: entry.canRevertSale
											? 'Undo the whole listing'
											: 'Undo the listing',
									entry.kind === 'conversion'
										? 'What it consumed comes back and what it produced is unmade.'
										: 'The listing goes entirely. Its stock returns and every fee it charged is unwritten.',
									entry,
									false,
								)}
							{/if}
						</div>
					{/if}
				</li>
			{/each}
		</ul>
	{/if}
</Modal>
