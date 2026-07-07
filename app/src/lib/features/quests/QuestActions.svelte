<script lang="ts">
	import { Button } from '$lib/components';
	import type { Quest } from '$lib/types';
	import type { CooldownStatus } from '$lib/types/common';

	let {
		quest,
		status,
		remaining,
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
		/** Whether this quest is showing the Keep/Undo reward cancel choice. */
		pendingCancelChoice: boolean;
		/** Quest-row style two-line remaining readout; playlist items use the inline span. */
		remainingDetail?: boolean;
		onStart: () => void;
		onComplete: () => void;
		onCancel: (undoReward: boolean) => void;
		onToggleCancelChoice: () => void;
	} = $props();
</script>

{#if status === 'cooling' && remaining}
	{#if remainingDetail}
		<div class="text-right">
			<div class="text-xs text-warning tabular-nums font-mono">{remaining}</div>
			<div class="text-[10px] text-text-tertiary">remaining</div>
		</div>
	{:else}
		<span class="text-xs text-warning tabular-nums font-mono">{remaining}</span>
	{/if}
	{#if pendingCancelChoice}
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
{:else if quest.startedAt}
	<Button size="sm" onclick={onComplete}>
		{#snippet children()}Complete{/snippet}
	</Button>
	<Button size="sm" variant="ghost" onclick={() => onCancel(false)}>
		{#snippet children()}Cancel{/snippet}
	</Button>
{:else}
	<Button size="sm" variant="secondary" onclick={onStart}>
		{#snippet children()}Start{/snippet}
	</Button>
{/if}
