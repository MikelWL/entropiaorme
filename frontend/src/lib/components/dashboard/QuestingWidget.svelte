<script lang="ts">
	import { Button, Badge, Select } from '$lib/components';
	import type { Quest, QuestPlaylist } from '$lib/types/quests';
	import type { CooldownStatus } from '$lib/types/common';

	export type PlaylistQuestItem = {
		quest: Quest;
		description: string | null;
		cd: CooldownStatus;
		inProgress: boolean;
	};

	let {
		playlists,
		activePlaylistId,
		activePlaylist,
		immediateItems,
		longHorizonItems,
		pendingCancelChoiceQuestId,
		copiedWp,
		onPlaylistChange,
		onQuestStart,
		onQuestComplete,
		onQuestCancel,
		onToggleCancelChoice,
		onCopyWaypoint,
		formatMinutes,
		getCooldownRemaining,
	}: {
		playlists: QuestPlaylist[];
		activePlaylistId: string | null;
		activePlaylist: QuestPlaylist | null;
		immediateItems: PlaylistQuestItem[];
		longHorizonItems: PlaylistQuestItem[];
		pendingCancelChoiceQuestId: string | null;
		copiedWp: string | null;
		onPlaylistChange: (id: string | null) => void;
		onQuestStart: (questId: string) => void;
		onQuestComplete: (questId: string) => void;
		onQuestCancel: (questId: string, undoReward: boolean) => void;
		onToggleCancelChoice: (questId: string) => void;
		onCopyWaypoint: (questId: string, waypoint: string) => void;
		formatMinutes: (m: number) => string;
		getCooldownRemaining: (quest: Quest) => string | null;
	} = $props();
</script>

<div class="flex-1 min-h-0 flex flex-col">
	{#if playlists.length === 0}
		<div class="flex-1 flex items-center justify-center">
			<p class="text-sm text-text-tertiary">Set up your quest playlist on the <a href="/quests" class="text-accent hover:underline">Quests page</a> to see it here.</p>
		</div>
	{:else}
		<!-- Playlist header -->
		<div class="relative flex items-center gap-2.5 mb-3">
			<Select
				class="flex-1"
				value={activePlaylistId ?? ''}
				onchange={(e) => onPlaylistChange(e.currentTarget.value || null)}
			>
				<option value="" disabled selected={!activePlaylistId}>Select a playlist…</option>
				{#each playlists as pl}
					<option value={pl.id}>{pl.name}</option>
				{/each}
			</Select>
			{#if activePlaylist}
				<span class="eyebrow tabular-nums">{formatMinutes(activePlaylist.estimatedMinutes)}</span>
			{/if}
		</div>

		<!-- Quest items -->
		{#if activePlaylistId}
			<div class="flex-1 min-h-0 overflow-y-auto space-y-1.5">
				{#if immediateItems.length > 0}
					<div class="eyebrow px-1 pt-1">Immediate quests</div>
					{#each immediateItems as item (item.quest.id)}
						{@const remaining = getCooldownRemaining(item.quest)}
						{#if item.description}
							<div class="text-xs text-text-secondary ml-1 pt-1">{item.description}</div>
						{/if}
						<div class="relative rounded-md border bg-surface/50 px-3 py-2 flex items-center gap-2.5
							transition-[border-color,background-color] duration-[var(--duration-base)] ease-[var(--ease-out)]
							{item.inProgress
								? 'border-accent/40 bg-accent-muted/10'
								: 'border-border/60 hover:border-border-bright/70'}
							{item.cd === 'cooling' ? 'opacity-55' : ''}">
							<span class="shrink-0
								{item.inProgress
									? 'signal-dot animate-pulse'
									: item.cd === 'ready' || item.cd === 'no_cooldown'
										? 'signal-dot positive'
										: 'signal-dot idle'}"></span>

							<div class="flex-1 min-w-0">
								<div class="flex items-center gap-2">
									<span class="text-sm font-medium truncate text-text tracking-tight">{item.quest.name}</span>
								</div>
								{#if item.quest.targetMobs.length > 0}
									<span class="text-xs text-text-tertiary">{item.quest.targetMobs.join(', ')}</span>
								{/if}
							</div>

							{#if item.cd === 'cooling' && remaining}
								<span class="text-xs text-warning tabular-nums font-medium shrink-0 tracking-wider">{remaining}</span>
							{:else if item.cd === 'ready'}
								<Badge variant="positive">{#snippet children()}Ready{/snippet}</Badge>
							{/if}

							{#if item.quest.waypoint}
								<button
									class="text-[10px] font-medium uppercase tracking-[0.14em] px-1.5 py-0.5 rounded
										text-accent cursor-pointer shrink-0
										transition-colors duration-[var(--duration-fast)]
										hover:bg-accent-muted/30"
									onclick={() => onCopyWaypoint(item.quest.id, item.quest.waypoint!)}
								>{copiedWp === item.quest.id ? 'Copied' : 'WP'}</button>
							{/if}

							<div class="shrink-0 flex items-center gap-1">
								{#if item.cd === 'cooling'}
									{#if pendingCancelChoiceQuestId === item.quest.id}
										<Button size="sm" variant="secondary" onclick={() => onQuestCancel(item.quest.id, false)}>
											{#snippet children()}Keep Reward{/snippet}
										</Button>
										<Button size="sm" variant="danger" onclick={() => onQuestCancel(item.quest.id, true)}>
											{#snippet children()}Undo Reward{/snippet}
										</Button>
									{:else}
										<Button size="sm" variant="ghost" onclick={() => onToggleCancelChoice(item.quest.id)}>
											{#snippet children()}Cancel{/snippet}
										</Button>
									{/if}
								{:else}
									{#if item.inProgress}
										<Button size="sm" onclick={() => onQuestComplete(item.quest.id)}>
											{#snippet children()}Complete{/snippet}
										</Button>
										<Button size="sm" variant="ghost" onclick={() => onQuestCancel(item.quest.id, false)}>
											{#snippet children()}Cancel{/snippet}
										</Button>
									{:else}
										<Button size="sm" variant="secondary" onclick={() => onQuestStart(item.quest.id)}>
											{#snippet children()}Start{/snippet}
										</Button>
									{/if}
								{/if}
							</div>
						</div>
					{/each}
				{/if}

				{#if longHorizonItems.length > 0}
					<div class="eyebrow px-1 pt-3">Long-horizon quests</div>
					{#each longHorizonItems as item (item.quest.id)}
						{@const remaining = getCooldownRemaining(item.quest)}
						{#if item.description}
							<div class="text-xs text-text-secondary ml-1 pt-1">{item.description}</div>
						{/if}
						<div class="relative rounded-md border bg-surface/30 px-3 py-2 flex items-center gap-2.5
							transition-[border-color,background-color] duration-[var(--duration-base)] ease-[var(--ease-out)]
							{item.inProgress
								? 'border-accent/40 bg-accent-muted/10'
								: 'border-border/40 hover:border-border-bright/60'}
							{item.cd === 'cooling' ? 'opacity-55' : ''}">
							<span class="shrink-0
								{item.inProgress
									? 'signal-dot animate-pulse'
									: item.cd === 'ready' || item.cd === 'no_cooldown'
										? 'signal-dot positive'
										: 'signal-dot idle'}"></span>

							<div class="flex-1 min-w-0">
								<div class="flex items-center gap-2">
									<span class="text-sm font-medium truncate text-text tracking-tight">{item.quest.name}</span>
									<Badge variant="neutral">{#snippet children()}Optional{/snippet}</Badge>
								</div>
								{#if item.quest.targetMobs.length > 0}
									<span class="text-xs text-text-tertiary">{item.quest.targetMobs.join(', ')}</span>
								{/if}
							</div>

							{#if item.cd === 'cooling' && remaining}
								<span class="text-xs text-warning tabular-nums font-medium shrink-0 tracking-wider">{remaining}</span>
							{:else if item.cd === 'ready'}
								<Badge variant="positive">{#snippet children()}Ready{/snippet}</Badge>
							{/if}

							{#if item.quest.waypoint}
								<button
									class="text-[10px] font-medium uppercase tracking-[0.14em] px-1.5 py-0.5 rounded
										text-accent cursor-pointer shrink-0
										transition-colors duration-[var(--duration-fast)]
										hover:bg-accent-muted/30"
									onclick={() => onCopyWaypoint(item.quest.id, item.quest.waypoint!)}
								>{copiedWp === item.quest.id ? 'Copied' : 'WP'}</button>
							{/if}

							<div class="shrink-0 flex items-center gap-1">
								{#if item.cd === 'cooling'}
									{#if pendingCancelChoiceQuestId === item.quest.id}
										<Button size="sm" variant="secondary" onclick={() => onQuestCancel(item.quest.id, false)}>
											{#snippet children()}Keep Reward{/snippet}
										</Button>
										<Button size="sm" variant="danger" onclick={() => onQuestCancel(item.quest.id, true)}>
											{#snippet children()}Undo Reward{/snippet}
										</Button>
									{:else}
										<Button size="sm" variant="ghost" onclick={() => onToggleCancelChoice(item.quest.id)}>
											{#snippet children()}Cancel{/snippet}
										</Button>
									{/if}
								{:else}
									{#if item.inProgress}
										<Button size="sm" onclick={() => onQuestComplete(item.quest.id)}>
											{#snippet children()}Complete{/snippet}
										</Button>
										<Button size="sm" variant="ghost" onclick={() => onQuestCancel(item.quest.id, false)}>
											{#snippet children()}Cancel{/snippet}
										</Button>
									{:else}
										<Button size="sm" variant="secondary" onclick={() => onQuestStart(item.quest.id)}>
											{#snippet children()}Start{/snippet}
										</Button>
									{/if}
								{/if}
							</div>
						</div>
					{/each}
				{/if}

				{#if immediateItems.length === 0 && longHorizonItems.length === 0 && activePlaylist}
					<div class="py-6 text-center text-sm text-text-tertiary">
						No quests in this playlist. <a href="/quests" class="text-accent hover:underline">Add quests</a>
					</div>
				{/if}
			</div>
		{:else}
			<div class="flex-1 flex items-center justify-center">
				<p class="text-sm text-text-tertiary">Select a playlist to view quests.</p>
			</div>
		{/if}
	{/if}
</div>
