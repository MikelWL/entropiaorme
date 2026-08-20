<script lang="ts">
	import Badge from '$lib/components/Badge.svelte';
	import Button from '$lib/components/Button.svelte';
	import type { ActivityOption, ActivityOptionsResult } from '$lib/api';
	import type { Quest } from '$lib/types/quests';

	let {
		activityOptions,
		quests,
		pendingCancelChoiceQuestId,
		copiedWp,
		onQuestStart,
		onQuestComplete,
		onQuestCancel,
		onToggleCancelChoice,
		onCopyWaypoint,
		onEditSession,
		getCooldownRemaining,
	}: {
		activityOptions: ActivityOptionsResult | null;
		quests: Quest[];
		pendingCancelChoiceQuestId: string | null;
		copiedWp: string | null;
		onQuestStart: (questId: string) => void;
		onQuestComplete: (questId: string) => void;
		onQuestCancel: (questId: string, undoReward: boolean) => void;
		onToggleCancelChoice: (questId: string) => void;
		onCopyWaypoint: (questId: string, waypoint: string) => void;
		onEditSession: (definitionId: number | null) => void;
		getCooldownRemaining: (quest: Quest) => string | null;
	} = $props();

	const roster = $derived(
		(activityOptions?.options ?? [])
			.filter((option) => option.kind !== 'segment' && option.rosterOrder != null && !option.offRoster)
			.sort((left, right) => (left.rosterOrder ?? 0) - (right.rosterOrder ?? 0)),
	);

	function questFor(option: ActivityOption): Quest | null {
		if (option.questId == null) return null;
		return quests.find((quest) => quest.id === String(option.questId)) ?? null;
	}
</script>

<div class="flex-1 min-h-0 flex flex-col">
	{#if !activityOptions?.definitionId}
		<div class="flex-1 flex items-center justify-center text-center">
			<div>
				<p class="text-sm text-text-tertiary">Choose a session type, then add its quests.</p>
				<Button class="mt-3" size="sm" variant="secondary" onclick={() => onEditSession(null)}>
					{#snippet children()}Create session{/snippet}
				</Button>
			</div>
		</div>
	{:else}
		<div class="flex items-baseline justify-between gap-3 pb-3 border-b border-border/60">
			<div>
				<div class="eyebrow">Session quests</div>
				<h3 class="mt-1 text-sm font-medium text-text">{activityOptions.definitionName}</h3>
			</div>
			<span class="text-xs tabular-nums text-text-tertiary">{roster.length}</span>
		</div>

		{#if roster.length === 0}
			<div class="flex-1 flex items-center justify-center text-center">
				<div>
					<p class="text-sm text-text-tertiary">No quests are attached to this session.</p>
					<Button class="mt-3" size="sm" variant="secondary" onclick={() => onEditSession(activityOptions.definitionId)}>
						{#snippet children()}Edit session{/snippet}
					</Button>
				</div>
			</div>
		{:else}
			<div class="flex-1 min-h-0 overflow-y-auto">
				{#each roster as option (option.key)}
					{@const quest = questFor(option)}
					{@const remaining = quest ? getCooldownRemaining(quest) : null}
					<div class="flex items-center gap-3 py-3 border-b border-border/45 last:border-b-0">
						<span class="shrink-0 {option.active ? 'signal-dot animate-pulse' : option.available ? 'signal-dot positive' : 'signal-dot idle'}"></span>
						<div class="flex-1 min-w-0">
							<div class="flex items-center gap-2 min-w-0">
								<span class="text-sm font-medium text-text truncate">{option.name}</span>
								{#if option.kind === 'quest_family'}
									<Badge variant="neutral">{#snippet children()}Family{/snippet}</Badge>
								{/if}
							</div>
							<div class="mt-0.5 text-xs text-text-tertiary">
								{#if option.handInWaiting}
									Waiting for the next reward clump
								{:else if option.active}
									Recording in this session
								{:else if !option.available}
									{option.unavailableReason}{remaining ? ` · ${remaining}` : ''}
								{:else if quest?.startedAt}
									In progress
								{:else}
									Ready
								{/if}
							</div>
						</div>

						{#if quest?.waypoint}
							<button
								type="button"
								class="text-[10px] font-medium uppercase tracking-[0.14em] px-1.5 py-0.5 text-accent hover:text-accent-hover"
								onclick={() => onCopyWaypoint(quest.id, quest.waypoint!)}
							>{copiedWp === quest.id ? 'Copied' : 'WP'}</button>
						{/if}

						{#if quest}
							<div class="shrink-0 flex items-center gap-1">
								{#if !option.available && !quest.startedAt && option.resettable}
									{#if pendingCancelChoiceQuestId === quest.id}
										<Button size="sm" variant="secondary" onclick={() => onQuestCancel(quest.id, false)}>{#snippet children()}Keep reward{/snippet}</Button>
										<Button size="sm" variant="danger" onclick={() => onQuestCancel(quest.id, true)}>{#snippet children()}Undo reward{/snippet}</Button>
									{:else}
										<Button size="sm" variant="ghost" onclick={() => onToggleCancelChoice(quest.id)}>{#snippet children()}Reset{/snippet}</Button>
									{/if}
								{:else if quest.startedAt}
									{#if quest.completionTrigger === 'manual_hand_in'}
										<span class="text-xs text-text-tertiary">Hand in from overlay</span>
									{:else}
										<Button size="sm" onclick={() => onQuestComplete(quest.id)}>{#snippet children()}Complete{/snippet}</Button>
									{/if}
									<Button size="sm" variant="ghost" onclick={() => onQuestCancel(quest.id, false)}>{#snippet children()}Cancel{/snippet}</Button>
								{:else if option.available}
									{#if quest.completionTrigger === 'manual_hand_in'}
										<span class="text-xs text-text-tertiary">Start from session Activities</span>
									{:else}
										<Button size="sm" variant="secondary" onclick={() => onQuestStart(quest.id)}>{#snippet children()}Start{/snippet}</Button>
									{/if}
									{#if quest.rewardUndoAvailable}
										{#if pendingCancelChoiceQuestId === quest.id}
											<Button size="sm" variant="danger" onclick={() => onQuestCancel(quest.id, true)}>{#snippet children()}Confirm undo{/snippet}</Button>
											<Button size="sm" variant="ghost" onclick={() => onToggleCancelChoice(quest.id)}>{#snippet children()}Back{/snippet}</Button>
										{:else}
											<Button size="sm" variant="ghost" onclick={() => onToggleCancelChoice(quest.id)}>{#snippet children()}Undo reward{/snippet}</Button>
										{/if}
									{/if}
								{/if}
							</div>
						{/if}
					</div>
				{/each}
			</div>
		{/if}
	{/if}
</div>
