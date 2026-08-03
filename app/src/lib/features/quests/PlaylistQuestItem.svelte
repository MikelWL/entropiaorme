<script lang="ts">
	import type { PlaylistItemGroup, Quest } from '$lib/types';
	import { getCooldownGate, getCooldownRemaining, getCooldownStatus } from './cooldown';
	import QuestActions from './QuestActions.svelte';
	import type { QuestsModel } from './questsModel.svelte';

	let {
		item,
		longHorizon = false,
		now,
		model
	}: {
		item: { quest: Quest; description: string | null; groupType: PlaylistItemGroup };
		/** Long-horizon items render dimmer and carry the Optional badge. */
		longHorizon?: boolean;
		now: number;
		model: QuestsModel;
	} = $props();

	const status = $derived(getCooldownStatus(item.quest, now));
	const remaining = $derived(getCooldownRemaining(item.quest, now));
	const familyGated = $derived(getCooldownGate(item.quest, now) === 'family');
</script>

{#if item.description}
	<div class="text-xs text-text-secondary ml-7 px-1 pt-1">{item.description}</div>
{/if}
<div class="flex items-center gap-2.5 {longHorizon ? 'bg-surface/35' : 'bg-surface/50'} rounded-md px-3 py-2">
	<span class="text-xs text-text-tertiary font-mono w-4 text-right shrink-0">{@html '&bull;'}</span>
	<div class="shrink-0">
		{#if item.quest.startedAt}
			<div class="w-2 h-2 rounded-full bg-accent animate-pulse"></div>
		{:else if status === 'ready' || status === 'no_cooldown'}
			<div class="w-2 h-2 rounded-full bg-success"></div>
		{:else}
			<div class="w-2 h-2 rounded-full bg-text-tertiary"></div>
		{/if}
	</div>
	<span class="text-sm text-text truncate flex-1">{item.quest.name}</span>
	{#if longHorizon}
		<span class="text-[10px] px-1.5 py-0.5 rounded-full bg-surface-hover text-text-tertiary border border-border/50 shrink-0">Optional</span>
	{/if}
	{#if item.quest.waypoint}
		<button
			class="text-[10px] text-accent hover:text-accent/80 transition-colors cursor-pointer shrink-0"
			onclick={() => model.copyWaypoint(item.quest.id, item.quest.waypoint!)}
		>{model.copiedWp === item.quest.id ? 'Copied!' : 'WP'}</button>
	{/if}
	<div class="shrink-0 flex items-center gap-1">
		<QuestActions
			quest={item.quest}
			{status}
			{remaining}
			{familyGated}
			pendingCancelChoice={model.pendingCancelChoiceQuestId === item.quest.id}
			onStart={() => model.handleStart(item.quest.id)}
			onComplete={() => model.handleComplete(item.quest.id)}
			onCancel={(undoReward) => model.handleCancel(item.quest.id, undoReward)}
			onToggleCancelChoice={() => model.toggleCancelChoice(item.quest.id)}
		/>
	</div>
</div>
