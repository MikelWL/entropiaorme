<script lang="ts">
	import { Button, Input, Modal, Select } from '$lib/components';
	import type { PlaylistModel } from './playlistModel.svelte';
	import { PLANETS } from './questsModel.svelte';

	let { model }: { model: PlaylistModel } = $props();
</script>

<Modal bind:open={model.showPlaylistModal} title={model.editingPlaylist ? 'Edit Playlist' : 'New Playlist'} class="max-w-lg">
	{#snippet children()}
		<form class="space-y-3" onsubmit={(e) => { e.preventDefault(); model.savePlaylist(); }}>
			<div class="grid grid-cols-2 gap-3">
				<div class="col-span-2">
					<label class="block text-xs text-text-secondary mb-1" for="pl-name">Name</label>
					<Input id="pl-name" type="text" required bind:value={model.playlistForm.name}
						placeholder="e.g., Quick Calypso Run" />
				</div>
				<div>
					<label class="block text-xs text-text-secondary mb-1" for="pl-planet">Planet</label>
					<Select id="pl-planet" bind:value={model.playlistForm.planet}>
						{#each PLANETS as planet}
							<option value={planet}>{planet}</option>
						{/each}
					</Select>
				</div>
				<div>
					<label class="block text-xs text-text-secondary mb-1" for="pl-time">Est. Time (min)</label>
					<Input id="pl-time" type="number" min="1" bind:value={model.playlistForm.estimated_minutes} />
				</div>
			</div>

			<!-- Immediate quests -->
			<div>
				<div class="block text-xs text-text-secondary mb-1.5">Immediate Quests</div>
				<p class="text-[11px] text-text-tertiary mb-2">These define the daily run and the playlist match requirement.</p>
				{#if model.playlistForm.immediate_items.length > 0}
					<div class="flex flex-col gap-1 mb-2">
						{#each model.playlistForm.immediate_items as item, i (item.quest_id)}
							<div class="flex items-center gap-2 bg-surface rounded-md px-3 py-1.5 text-sm">
								<span class="text-text-tertiary text-xs font-mono w-4 text-right">{i + 1}</span>
								<span class="flex-1 text-text truncate">{model.questName(item.quest_id)}</span>
								<button type="button" class="text-[10px] text-text-tertiary hover:text-accent cursor-pointer" onclick={() => model.moveQuestBetweenGroups(item.quest_id, 'immediate')}>Long</button>
								<button type="button" class="text-text-tertiary hover:text-text cursor-pointer disabled:opacity-30" disabled={i === 0} onclick={() => model.moveQuestUp('immediate', i)}>&#x25B2;</button>
								<button type="button" class="text-text-tertiary hover:text-text cursor-pointer disabled:opacity-30" disabled={i >= model.playlistForm.immediate_items.length - 1} onclick={() => model.moveQuestDown('immediate', i)}>&#x25BC;</button>
								<button type="button" class="text-text-tertiary hover:text-negative cursor-pointer" onclick={() => model.removeQuestFromPlaylist(item.quest_id, 'immediate')}>×</button>
							</div>
						{/each}
					</div>
				{/if}
			</div>

			<!-- Long-horizon quests -->
			<div>
				<div class="block text-xs text-text-secondary mb-1.5">Long-Horizon Quests</div>
				<p class="text-[11px] text-text-tertiary mb-2">These may complete during the run, but they are optional for playlist matching.</p>
				{#if model.playlistForm.long_horizon_items.length > 0}
					<div class="flex flex-col gap-1 mb-2">
						{#each model.playlistForm.long_horizon_items as item, i (item.quest_id)}
							<div class="flex items-center gap-2 bg-surface rounded-md px-3 py-1.5 text-sm">
								<span class="text-text-tertiary text-xs font-mono w-4 text-right">{i + 1}</span>
								<span class="flex-1 text-text truncate">{model.questName(item.quest_id)}</span>
								<button type="button" class="text-[10px] text-text-tertiary hover:text-accent cursor-pointer" onclick={() => model.moveQuestBetweenGroups(item.quest_id, 'long_horizon')}>Immediate</button>
								<button type="button" class="text-text-tertiary hover:text-text cursor-pointer disabled:opacity-30" disabled={i === 0} onclick={() => model.moveQuestUp('long_horizon', i)}>&#x25B2;</button>
								<button type="button" class="text-text-tertiary hover:text-text cursor-pointer disabled:opacity-30" disabled={i >= model.playlistForm.long_horizon_items.length - 1} onclick={() => model.moveQuestDown('long_horizon', i)}>&#x25BC;</button>
								<button type="button" class="text-text-tertiary hover:text-negative cursor-pointer" onclick={() => model.removeQuestFromPlaylist(item.quest_id, 'long_horizon')}>×</button>
							</div>
						{/each}
					</div>
				{/if}
			</div>

			<!-- Available quests -->
			<div>
				<div class="block text-xs text-text-secondary mb-1.5">Add Quests</div>
				{#if model.availableForPlaylist.length > 0}
					<div class="border border-border rounded-md max-h-48 overflow-y-auto">
						{#each model.availableForPlaylist as quest (quest.id)}
							<div class="w-full flex items-center gap-2 px-3 py-1.5 text-sm text-left text-text-secondary border-b border-border/30 last:border-b-0">
								<span class="truncate flex-1">{quest.name}</span>
								<span class="text-xs text-text-tertiary shrink-0">{quest.planet}</span>
								<button
									type="button"
									class="text-[10px] px-2 py-1 rounded border border-accent/25 text-accent hover:bg-accent/10 transition-colors cursor-pointer"
									onclick={() => model.addQuestToPlaylist(quest.id, 'immediate')}
								>+ Immediate</button>
								<button
									type="button"
									class="text-[10px] px-2 py-1 rounded border border-border/60 text-text-tertiary hover:text-text hover:bg-surface-hover transition-colors cursor-pointer"
									onclick={() => model.addQuestToPlaylist(quest.id, 'long_horizon')}
								>+ Long</button>
							</div>
						{/each}
					</div>
				{:else}
					<div class="text-xs text-text-tertiary rounded-md border border-border/50 px-3 py-2">
						All active quests are already in this playlist.
					</div>
				{/if}
			</div>

			<div class="flex justify-end gap-2 pt-1">
				<Button type="button" variant="ghost" onclick={() => (model.showPlaylistModal = false)}>{#snippet children()}Cancel{/snippet}</Button>
				<Button type="submit">{#snippet children()}{model.editingPlaylist ? 'Save' : 'Create'}{/snippet}</Button>
			</div>
		</form>
	{/snippet}
</Modal>
