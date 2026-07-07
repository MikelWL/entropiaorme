<script lang="ts">
	import { Badge, Button, Menu } from '$lib/components';
	import { formatMinutes } from './cooldown';
	import type { PlaylistModel } from './playlistModel.svelte';
	import PlaylistQuestItem from './PlaylistQuestItem.svelte';
	import type { QuestsModel } from './questsModel.svelte';

	let {
		model,
		questsModel,
		now
	}: {
		model: PlaylistModel;
		questsModel: QuestsModel;
		now: number;
	} = $props();
</script>

<div data-guide-anchor="quests-playlists-view">
{#if questsModel.playlists.length === 0}
	<div class="text-center py-8 text-sm text-text-tertiary">
		No playlists yet. Create one to organise your quest rotation.
	</div>
{:else}
	<div class="space-y-2">
		{#each questsModel.playlists as pl (pl.id)}
			{@const isExpanded = model.expandedPlaylistId === pl.id}
			{@const allReady = model.playlistAllReady(pl, now)}
			{@const immediateItems = model.playlistQuestItems(pl, 'immediate')}
			{@const longHorizonItems = model.playlistQuestItems(pl, 'long_horizon')}
			<div class="bg-surface-raised/50 rounded-lg border border-border/50 hover:bg-surface-raised/70 transition-colors">
				<!-- Playlist header -->
				<div class="flex items-center px-4 py-3">
					<button
						class="flex-1 flex items-center gap-2.5 text-left cursor-pointer min-w-0"
						onclick={() => (model.expandedPlaylistId = isExpanded ? null : pl.id)}
					>
						<!-- Time badge -->
						<span class="text-[10px] font-medium px-1.5 py-0.5 rounded-full border shrink-0
							{pl.estimatedMinutes <= 10 ? 'bg-success/10 text-success border-success/20' :
							 pl.estimatedMinutes <= 30 ? 'bg-warning/10 text-warning border-warning/20' :
							 'bg-negative/10 text-negative border-negative/20'}">
							{formatMinutes(pl.estimatedMinutes)}
						</span>
						<span class="text-sm font-medium text-text truncate">{pl.name}</span>
						<span class="text-xs text-text-tertiary shrink-0">{immediateItems.length} immediate</span>
						{#if longHorizonItems.length > 0}
							<span class="text-xs text-text-tertiary shrink-0">+ {longHorizonItems.length} long</span>
						{/if}
						{#if allReady && immediateItems.length > 0}
							<Badge variant="positive">{#snippet children()}Ready{/snippet}</Badge>
						{/if}
						<svg
							class="w-3.5 h-3.5 text-text-tertiary transition-transform ml-auto shrink-0 {isExpanded ? 'rotate-180' : ''}"
							fill="none" stroke="currentColor" viewBox="0 0 24 24"
						>
							<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
						</svg>
					</button>
					<!-- Three-dot menu -->
					<Menu
						class="ml-2 shrink-0"
						ariaLabel="Playlist actions"
						items={[{ label: 'Edit', onSelect: () => model.openEditPlaylist(pl) }]}
					>
						{#if questsModel.deleteConfirmId === `pl-${pl.id}`}
							<div class="flex gap-1 px-2 py-1">
								<Button class="flex-1" size="sm" variant="danger" onclick={() => model.handleDeletePlaylist(pl.id)}>
									{#snippet children()}Confirm{/snippet}
								</Button>
								<Button class="flex-1" size="sm" variant="ghost" onclick={() => (questsModel.deleteConfirmId = null)}>
									{#snippet children()}Cancel{/snippet}
								</Button>
							</div>
						{:else}
							<button
								role="menuitem"
								tabindex="-1"
								class="w-full px-3 py-1.5 text-xs text-left text-text-secondary hover:bg-surface-hover hover:text-negative cursor-pointer"
								onclick={() => (questsModel.deleteConfirmId = `pl-${pl.id}`)}
							>Delete</button>
						{/if}
					</Menu>
				</div>

				<!-- Expanded playlist items -->
				{#if isExpanded}
					<div class="border-t border-border/50 px-3 pb-3 pt-2 space-y-1.5">
						{#if immediateItems.length > 0}
							<div class="space-y-1.5">
								<div class="px-1 pt-1 eyebrow">Immediate Quests</div>
								{#each immediateItems as item (item.quest.id)}
									<PlaylistQuestItem {item} {now} model={questsModel} />
								{/each}
							</div>
						{/if}
						{#if longHorizonItems.length > 0}
							<div class="space-y-1.5 pt-2">
								<div class="px-1 pt-1 eyebrow">Long-Horizon Quests</div>
								{#each longHorizonItems as item (item.quest.id)}
									<PlaylistQuestItem {item} longHorizon {now} model={questsModel} />
								{/each}
							</div>
						{/if}
						{#if immediateItems.length === 0 && longHorizonItems.length === 0}
							<p class="text-xs text-text-tertiary py-2 text-center">No quests in this playlist.</p>
						{/if}
					</div>
				{/if}
			</div>
		{/each}
	</div>
{/if}
</div>
