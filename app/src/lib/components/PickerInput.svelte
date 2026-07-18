<script lang="ts" generics="T">
	import type { Snippet } from 'svelte';
	import SearchInput from './SearchInput.svelte';

	/**
	 * Structural contract for the typeahead model this presenter renders.
	 * A `createTypeahead` instance satisfies it; the shape is declared here
	 * so the presenter stays decoupled from any one factory.
	 */
	interface PickerModel {
		query: string;
		results: T[];
		selected: T | null;
		loading: boolean;
		error: string | null;
		select(item: T): void;
		clear(): void;
	}

	let {
		id,
		placeholder,
		model,
		result,
		selection,
		extraRow,
		class: className = '',
		dropdownClass = '',
	}: {
		id: string;
		placeholder?: string;
		model: PickerModel;
		/** Renders one dropdown row for a search hit. */
		result: Snippet<[{ item: T }]>;
		/** Renders the selected chip's content; `clear` releases the selection. */
		selection: Snippet<[{ item: T; clear: () => void }]>;
		/** Optional trailing dropdown row (e.g. an "Add custom" affordance). */
		extraRow?: Snippet;
		class?: string;
		dropdownClass?: string;
	} = $props();

	const listboxId = $derived(`${id}-listbox`);

	let activeIndex = $state(-1);
	// Escape dismisses the dropdown for the query it was pressed on; editing
	// the query brings it back. The presenter hides rows rather than mutating
	// the model's result list, which it does not own.
	let dismissedForQuery = $state<string | null>(null);

	const showDropdown = $derived(
		!model.selected &&
			dismissedForQuery !== model.query &&
			(model.results.length > 0 || (!!extraRow && model.query.trim().length > 0)),
	);

	// A fresh result set restarts the highlight.
	$effect(() => {
		void model.results;
		activeIndex = -1;
	});

	function handleKeydown(e: KeyboardEvent) {
		if (!showDropdown) return;
		const count = model.results.length;
		switch (e.key) {
			case 'ArrowDown':
				e.preventDefault();
				if (count > 0) activeIndex = (activeIndex + 1) % count;
				break;
			case 'ArrowUp':
				e.preventDefault();
				if (count > 0) activeIndex = activeIndex <= 0 ? count - 1 : activeIndex - 1;
				break;
			case 'Enter':
				if (count > 0) {
					e.preventDefault();
					model.select(model.results[activeIndex >= 0 ? activeIndex : 0]);
				}
				break;
			case 'Escape':
				e.preventDefault();
				dismissedForQuery = model.query;
				activeIndex = -1;
				break;
		}
	}
</script>

<div class={className}>
	<SearchInput
		{id}
		{placeholder}
		bind:value={model.query}
		loading={model.loading}
		role="combobox"
		aria-expanded={showDropdown}
		aria-autocomplete="list"
		aria-controls={showDropdown ? listboxId : undefined}
		aria-activedescendant={activeIndex >= 0 ? `${id}-option-${activeIndex}` : undefined}
		onkeydown={handleKeydown}
	/>

	{#if showDropdown}
		<div
			id={listboxId}
			role="listbox"
			aria-label="Suggestions"
			class="mt-1 bg-surface border border-border rounded-md overflow-hidden max-h-48 overflow-y-auto {dropdownClass}"
		>
			{#each model.results as item, i}
				<button
					id="{id}-option-{i}"
					role="option"
					aria-selected={i === activeIndex}
					tabindex="-1"
					class="w-full text-left px-3 py-2 text-sm hover:bg-surface-hover
						transition-colors duration-[var(--duration-fast)] cursor-pointer
						flex items-center justify-between {i === activeIndex ? 'bg-surface-hover' : ''}"
					onclick={() => model.select(item)}
				>
					{@render result({ item })}
				</button>
			{/each}
			{#if extraRow}
				{@render extraRow()}
			{/if}
		</div>
	{/if}

	{#if model.selected}
		<div class="mt-2 px-3 py-2 bg-surface rounded-md border border-border/50 text-sm">
			{@render selection({ item: model.selected, clear: () => model.clear() })}
		</div>
	{/if}

	{#if model.error}
		<p class="mt-1 text-xs text-negative">{model.error}</p>
	{/if}
</div>
