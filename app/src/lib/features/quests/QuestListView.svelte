<script lang="ts">
	import { SearchInput, Select } from '$lib/components';
	import QuestRow from './QuestRow.svelte';
	import type { QuestsModel } from './questsModel.svelte';

	let { model, now }: { model: QuestsModel; now: number } = $props();
</script>

<!-- Filters row -->
<div class="flex flex-wrap items-center gap-4">
	<!-- Planet filter -->
	{#if model.planets.length > 1}
		<div class="flex items-center gap-2">
			<label for="planet-select" class="text-xs text-text-tertiary uppercase tracking-wide">Planet</label>
			<Select
				id="planet-select"
				class="min-w-[120px]"
				bind:value={model.selectedPlanet}
				onchange={() => (model.selectedMob = null)}
			>
				<option value={null}>All Planets</option>
				{#each model.planets as planet}
					<option value={planet}>{planet}</option>
				{/each}
			</Select>
		</div>
	{/if}

	<!-- Mob filter -->
	{#if model.mobs.length > 0}
		<div class="flex items-center gap-2">
			<label for="mob-select" class="text-xs text-text-tertiary uppercase tracking-wide">Mob</label>
			<Select
				id="mob-select"
				class="min-w-[120px]"
				bind:value={model.selectedMob}
			>
				<option value={null}>All Mobs</option>
				{#each model.mobs as mob}
					<option value={mob}>{mob}</option>
				{/each}
			</Select>
		</div>
	{/if}

	<!-- Search -->
	<div class="flex-1 min-w-[200px]">
		<SearchInput bind:value={model.searchQuery} placeholder="Search by name, mob, category..." />
	</div>
</div>

{#if model.filteredQuests.length === 0}
	<div class="text-center py-8 text-sm text-text-tertiary">
		{model.searchQuery ? `No quests match "${model.searchQuery}"` : 'No quests yet. Add your first quest to get started.'}
	</div>
{:else}
	<div class="space-y-4">
		{#each model.questsByCategory as group (group.category)}
			{@const isCollapsed = model.collapsedCategories.has(group.category)}
			{@const counts = model.categoryStatusCounts(group.quests, now)}

			{#if group.category}
				<!-- Category section -->
				<div class="rounded-lg border border-border/50 overflow-hidden">
					<!-- Category header -->
					<button
						class="w-full flex items-center gap-2.5 py-2.5 px-4 text-left cursor-pointer
							bg-surface-raised/60 hover:bg-surface-raised/80 transition-colors"
						onclick={() => {
							const next = new Set(model.collapsedCategories);
							if (isCollapsed) next.delete(group.category);
							else next.add(group.category);
							model.collapsedCategories = next;
						}}
					>
						<svg
							class="w-3.5 h-3.5 text-text-tertiary transition-transform shrink-0 {isCollapsed ? '-rotate-90' : ''}"
							fill="none" stroke="currentColor" viewBox="0 0 24 24"
						>
							<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
						</svg>
						<span class="text-sm font-semibold text-text">{group.category}</span>
						<span class="text-xs text-text-tertiary">{group.quests.length}</span>
						<div class="flex items-center gap-1.5 ml-auto">
							{#if counts.ready > 0}
								<span class="text-[10px] px-1.5 py-0.5 rounded-full bg-success/10 text-success border border-success/20">
									{counts.ready} ready
								</span>
							{/if}
							{#if counts.started > 0}
								<span class="text-[10px] px-1.5 py-0.5 rounded-full bg-accent/10 text-accent border border-accent/20">
									{counts.started} started
								</span>
							{/if}
							{#if counts.cooling > 0}
								<span class="text-[10px] px-1.5 py-0.5 rounded-full bg-warning/10 text-warning border border-warning/20">
									{counts.cooling} on cd
								</span>
							{/if}
						</div>
					</button>

					<!-- Category quests -->
					{#if !isCollapsed}
						<div class="space-y-1.5 p-2">
							{#each group.quests as quest (quest.id)}
								<QuestRow {quest} {now} {model} />
							{/each}
						</div>
					{/if}
				</div>
			{:else}
				<!-- Uncategorised quests (no wrapper) -->
				<div class="space-y-1.5">
					{#each group.quests as quest (quest.id)}
						<QuestRow {quest} {now} {model} />
					{/each}
				</div>
			{/if}
		{/each}
	</div>
{/if}
