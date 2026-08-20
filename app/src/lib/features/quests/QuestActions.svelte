<script lang="ts">
	import Button from '$lib/components/Button.svelte';
	import type { Quest } from '$lib/types';
	import type { CooldownStatus } from '$lib/types/common';

	let {
		quest,
		status,
		remaining,
		familyGated = false,
		pendingCancelChoice,
		remainingDetail = false,
		onStart,
		onComplete,
		onCancel,
		onToggleCancelChoice
	}: {
		quest: Quest;
		status: CooldownStatus;
		remaining: string | null;
		/** Cooling held ONLY by the family window: show the wait, but no
		 * Cancel (a member action must never eat the family's timer). */
		familyGated?: boolean;
		/** Whether this quest is showing the Keep/Undo reward cancel choice. */
		pendingCancelChoice: boolean;
		/** Quest-row style two-line remaining readout. */
		remainingDetail?: boolean;
		onStart: () => void;
		onComplete: () => void;
		onCancel: (undoReward: boolean) => void;
		onToggleCancelChoice: () => void;
	} = $props();
</script>

<!-- An in-progress quest leads: under a pickup anchor a started quest
	 can be cooling at the same time (its own or its family's timer runs
	 from the start), and Complete must stay reachable through it. -->
{#if quest.startedAt}
	{#if quest.completionTrigger === 'manual_hand_in'}
		<span class="text-xs text-text-tertiary">Hand in from overlay</span>
	{:else}
		<Button size="sm" onclick={onComplete}>
			{#snippet children()}Complete{/snippet}
		</Button>
	{/if}
	<Button size="sm" variant="ghost" onclick={() => onCancel(false)}>
		{#snippet children()}Cancel{/snippet}
	</Button>
{:else if status === 'cooling' && remaining}
	{#if remainingDetail}
		<div class="text-right">
			<div class="text-xs text-warning tabular-nums font-mono">{remaining}</div>
			<div class="text-[10px] text-text-tertiary">{familyGated ? 'family cd' : 'remaining'}</div>
		</div>
	{:else}
		<span class="text-xs text-warning tabular-nums font-mono" title={familyGated ? 'Held by the family cooldown' : undefined}>{remaining}</span>
	{/if}
	{#if familyGated}
		<!-- No Cancel: the wait belongs to the family, and resetting it is
			 a family-level decision, not a row action. Start stays absent
			 too: the giver will not offer this slot while it cools. -->
	{:else if pendingCancelChoice}
		<Button size="sm" variant="secondary" onclick={() => onCancel(false)}>
			{#snippet children()}Keep Reward{/snippet}
		</Button>
		<Button size="sm" variant="danger" onclick={() => onCancel(true)}>
			{#snippet children()}Undo Reward{/snippet}
		</Button>
	{:else}
		<Button size="sm" variant="ghost" onclick={onToggleCancelChoice}>
			{#snippet children()}Cancel{/snippet}
		</Button>
	{/if}
{:else}
	{#if quest.completionTrigger === 'manual_hand_in'}
		<span class="text-xs text-text-tertiary">Start from session Activities</span>
	{:else}
		<Button size="sm" variant="secondary" onclick={onStart}>
			{#snippet children()}Start{/snippet}
		</Button>
	{/if}
	{#if quest.rewardUndoAvailable}
		{#if pendingCancelChoice}
			<Button size="sm" variant="danger" onclick={() => onCancel(true)}>
				{#snippet children()}Confirm undo{/snippet}
			</Button>
			<Button size="sm" variant="ghost" onclick={onToggleCancelChoice}>
				{#snippet children()}Back{/snippet}
			</Button>
		{:else}
			<Button size="sm" variant="ghost" onclick={onToggleCancelChoice}>
				{#snippet children()}Undo reward{/snippet}
			</Button>
		{/if}
	{/if}
{/if}
