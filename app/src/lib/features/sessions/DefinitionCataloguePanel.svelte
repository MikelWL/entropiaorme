<script lang="ts">
	import type { Snippet } from 'svelte';
	import SearchInput from '$lib/components/SearchInput.svelte';

	let {
		title,
		count,
		filter = $bindable(''),
		hasMatches,
		filterLabel,
		resultsTestId,
		results,
		footer
	}: {
		title: string;
		count: number;
		filter?: string;
		hasMatches: boolean;
		filterLabel: string;
		resultsTestId?: string;
		results: Snippet;
		footer?: Snippet;
	} = $props();
</script>

<div class="flex max-h-[min(30rem,calc(100vh-1rem))] flex-col">
	<div class="flex flex-col gap-2 border-b border-border/60 p-3">
		<div class="flex items-baseline justify-between gap-3">
			<span class="text-sm font-semibold text-text">{title}</span>
			<span class="text-xs tabular-nums text-text-tertiary">{count}</span>
		</div>
		<SearchInput
			bind:value={filter}
			placeholder="Filter sessions..."
			aria-label={filterLabel}
			autocomplete="off"
			spellcheck={false}
		/>
	</div>

	<div
		class="min-h-0 flex-1 overflow-y-auto overscroll-contain p-1"
		data-testid={resultsTestId}
	>
		{@render results()}
		{#if !hasMatches}
			<p class="px-3 py-8 text-center text-sm text-text-tertiary">
				No sessions match “{filter.trim()}”.
			</p>
		{/if}
	</div>

	{#if footer}
		<div class="flex items-center justify-between gap-2 border-t border-border/60 p-2">
			{@render footer()}
		</div>
	{/if}
</div>
