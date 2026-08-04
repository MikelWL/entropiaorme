<script lang="ts">
	import type { DefinitionsModel } from './definitionsModel.svelte';

	let {
		model,
		selectedId = null,
		locked = false,
		onOpenAuthoring
	}: {
		model: DefinitionsModel;
		/** The definition currently selected for the next session (the
		 * snapshot's `sessionDefinitionId`), or the running session's. */
		selectedId?: string | null;
		/** While a session runs the selection is fixed: the chips render
		 * the fact without offering the switch. */
		locked?: boolean;
		/** Open the authoring environment; the trigger's rect feeds the
		 * morph, and a definition means edit-in-place. */
		onOpenAuthoring: (rect: DOMRect, definitionId: string | null) => void;
	} = $props();

	function triggerRect(event: MouseEvent): DOMRect {
		return (event.currentTarget as HTMLElement).getBoundingClientRect();
	}
</script>

<div class="flex flex-col gap-1.5" data-guide-anchor="dashboard-session-types">
	<span class="eyebrow">Session type</span>
	<div class="flex flex-wrap items-center gap-1.5">
		{#each model.definitions as definition (definition.id)}
			{@const selected = selectedId === definition.id}
			<span class="inline-flex items-stretch">
				<button
					type="button"
					class="filter-chip {selected ? 'is-active' : ''} {selected && !locked
						? 'rounded-r-none'
						: ''}"
					disabled={model.selecting || (locked && !selected)}
					aria-pressed={selected}
					title={locked
						? selected
							? `${definition.name} (fixed for the running session)`
							: 'The session type is fixed while a session runs'
						: selected
							? 'Selected for the next session; click to clear'
							: 'Select for the next session'}
					onclick={() => {
						if (locked) return;
						void model.select(selected ? null : definition.id);
					}}
				>
					{definition.name}
				</button>
				{#if selected && !locked}
					<button
						type="button"
						class="filter-chip is-active rounded-l-none border-l-0 px-1.5"
						aria-label={`Edit ${definition.name}`}
						title="Edit this session type"
						disabled={model.selecting}
						onclick={(event) => onOpenAuthoring(triggerRect(event), definition.id)}
					>
						<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="h-3 w-3">
							<path d="M13.586 3.586a2 2 0 112.828 2.828l-.793.793-2.828-2.828.793-.793zM11.379 5.793L3 14.172V17h2.828l8.38-8.379-2.83-2.828z" />
						</svg>
					</button>
				{/if}
			</span>
		{/each}
		{#if !locked}
			<button
				type="button"
				class="filter-chip"
				title="Create a session type"
				disabled={model.selecting}
				onclick={(event) => onOpenAuthoring(triggerRect(event), null)}
			>
				+ New
			</button>
		{/if}
		{#if model.definitions.length === 0 && !model.loading && !locked}
			<span class="text-xs text-text-tertiary">
				Define the kinds of session you play; instances group under them.
			</span>
		{/if}
	</div>
</div>
