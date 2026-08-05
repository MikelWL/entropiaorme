<script lang="ts">
	import Menu from '$lib/components/Menu.svelte';
	import DefinitionCataloguePanel from './DefinitionCataloguePanel.svelte';
	import { filterDefinitions } from './definitionCatalogue';
	import type { ReviewModel } from './reviewModel.svelte';

	let { model }: { model: ReviewModel } = $props();

	let filter = $state('');
	const active = $derived(filterDefinitions(model.activeDefinitions, filter));
	const archived = $derived(filterDefinitions(model.archivedDefinitions, filter));
	const total = $derived(model.activeDefinitions.length + model.archivedDefinitions.length);
</script>

<Menu
	ariaLabel="Review another session"
	overlay
	align="left"
	initialFocus="first-input"
	overlayOverflow="hidden"
	panelClass="w-[min(24rem,calc(100vw-1rem))] p-0"
>
	{#snippet trigger({ open, toggle, keydown })}
		<button
			type="button"
			class="inline-flex max-w-[20rem] items-center gap-1 rounded-md px-1.5 py-0.5
				cursor-pointer text-sm font-semibold tracking-tight text-accent
				transition-colors duration-[var(--duration-fast)] hover:bg-surface-hover"
			aria-haspopup="menu"
			aria-expanded={open}
			aria-label={model.definition
				? `Review another session (currently ${model.definition.name})`
				: 'Choose a session to review'}
			onclick={() => {
				if (!open) filter = '';
				toggle();
			}}
			onkeydown={(event) => {
				if (!open && event.key === 'ArrowDown') filter = '';
				keydown(event);
			}}
		>
			<span class="truncate">{model.definition?.name ?? 'Choose a session'}</span>
			<span class="text-text-secondary" aria-hidden="true">&#x2304;</span>
		</button>
	{/snippet}

	{#snippet children({ close })}
		<DefinitionCataloguePanel
			title="Review session"
			count={total}
			bind:filter
			hasMatches={active.length > 0 || archived.length > 0}
			filterLabel="Filter review sessions"
		>
			{#snippet results()}
				{#if active.length > 0}
					<p class="eyebrow px-2 pb-1 pt-1.5">Active</p>
					{#each active as definition (definition.id)}
						<button
							type="button"
							role="menuitem"
							aria-current={definition.id === model.definitionId ? 'true' : undefined}
							class="mt-0.5 flex w-full items-center gap-2 rounded px-2.5 py-2 text-left text-sm
								cursor-pointer transition-colors duration-[var(--duration-fast)]
								{definition.id === model.definitionId
								? 'bg-accent/10 text-accent'
								: 'text-text-secondary hover:bg-surface-hover hover:text-text'}"
							onclick={() => {
								close();
								void model.reviewDefinition(definition.id);
							}}
						>
							<span class="min-w-0 flex-1 truncate">{definition.name}</span>
							<span class="shrink-0 text-xs tabular-nums text-text-tertiary">
								{definition.instanceCount}
							</span>
						</button>
					{/each}
				{/if}

				{#if archived.length > 0}
					<p class="eyebrow mt-2 border-t border-border/50 px-2 pb-1 pt-2">Archived</p>
					{#each archived as definition (definition.id)}
						<button
							type="button"
							role="menuitem"
							aria-current={definition.id === model.definitionId ? 'true' : undefined}
							class="mt-0.5 flex w-full items-center gap-2 rounded px-2.5 py-2 text-left text-sm
								cursor-pointer text-text-tertiary transition-colors duration-[var(--duration-fast)]
								hover:bg-surface-hover hover:text-text-secondary
								{definition.id === model.definitionId ? 'bg-surface-hover' : ''}"
							onclick={() => {
								close();
								void model.reviewDefinition(definition.id);
							}}
						>
							<span class="min-w-0 flex-1 truncate">{definition.name}</span>
							<span class="shrink-0 text-xs tabular-nums">{definition.instanceCount}</span>
						</button>
					{/each}
				{/if}
			{/snippet}
		</DefinitionCataloguePanel>
	{/snippet}
</Menu>
