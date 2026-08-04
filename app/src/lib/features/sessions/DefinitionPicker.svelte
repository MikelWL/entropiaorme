<script lang="ts">
	import Menu from '$lib/components/Menu.svelte';
	import type { DefinitionsModel } from './definitionsModel.svelte';

	let {
		model,
		selectedId = null,
		onOpenAuthoring
	}: {
		model: DefinitionsModel;
		/** The session selected for the next run (the snapshot's
		 * `sessionDefinitionId`), carried in configuration so it survives
		 * app restarts and reads as the last session played. */
		selectedId?: string | null;
		/** Open the authoring environment; an id means edit-in-place,
		 * null means create. */
		onOpenAuthoring: (definitionId: string | null) => void;
	} = $props();

	const selected = $derived(
		model.definitions.find((definition) => definition.id === selectedId) ?? null
	);
</script>

<!-- The island's title: the session it runs as, switched in place. -->
<div class="flex items-center gap-1.5 min-w-0" data-guide-anchor="dashboard-session">
	<h2 class="text-[15px] font-semibold text-text tracking-tight shrink-0">Session:</h2>

	{#if model.definitions.length > 0}
		<Menu ariaLabel="Switch session" panelClass="left-0 right-auto top-9 w-60 p-1">
			{#snippet trigger({ open, toggle })}
				<button
					type="button"
					class="inline-flex max-w-[18rem] items-center gap-1 rounded-md px-1.5 py-0.5 cursor-pointer
						text-[15px] font-semibold tracking-tight
						transition-colors duration-[var(--duration-fast)] hover:bg-surface-hover
						disabled:cursor-not-allowed disabled:opacity-60
						{selected ? 'text-accent' : 'text-text-tertiary hover:text-text'}"
					aria-haspopup="menu"
					aria-expanded={open}
					aria-label={selected ? `Switch session (currently ${selected.name})` : 'Choose a session'}
					title="Switch session"
					disabled={model.selecting}
					onclick={toggle}
				>
					<span class="truncate">{selected ? selected.name : 'Choose'}</span>
					<span class="text-text-secondary" aria-hidden="true">&#x2304;</span>
				</button>
			{/snippet}

			{#snippet children({ close })}
				{#each model.definitions as definition (definition.id)}
					<div
						class="mt-0.5 flex items-center gap-1 rounded {selectedId === definition.id
							? 'bg-accent/10'
							: ''}"
					>
						<button
							type="button"
							role="menuitem"
							class="min-w-0 flex-1 truncate px-2 py-1.5 text-left text-sm cursor-pointer
								{selectedId === definition.id
								? 'text-accent'
								: 'text-text-secondary hover:text-text'}"
							onclick={() => {
								if (selectedId !== definition.id) void model.select(definition.id);
								close();
							}}
						>
							{definition.name}
						</button>
						<button
							type="button"
							role="menuitem"
							class="h-7 w-7 shrink-0 rounded cursor-pointer text-text-secondary
								hover:bg-surface-hover hover:text-text"
							aria-label={`Edit ${definition.name}`}
							title="Edit"
							onclick={() => {
								close();
								onOpenAuthoring(definition.id);
							}}
						>
							<svg
								xmlns="http://www.w3.org/2000/svg"
								viewBox="0 0 20 20"
								fill="currentColor"
								class="h-3 w-3 mx-auto"
								aria-hidden="true"
							>
								<path
									d="M13.586 3.586a2 2 0 112.828 2.828l-.793.793-2.828-2.828.793-.793zM11.379 5.793L3 14.172V17h2.828l8.38-8.379-2.83-2.828z"
								/>
							</svg>
						</button>
					</div>
				{/each}

			{/snippet}
		</Menu>
	{/if}

	<!-- Sits beside the name, and stands alone before the list loads. -->
	<button
		type="button"
		class="filter-chip shrink-0"
		title="Create a session"
		disabled={model.selecting}
		onclick={() => onOpenAuthoring(null)}
	>
		+ New
	</button>
</div>
