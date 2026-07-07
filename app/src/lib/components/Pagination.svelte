<script lang="ts">
	import Button from './Button.svelte';

	let {
		page = $bindable(0),
		totalPages,
		rangeLabel,
		class: className = ''
	}: {
		/** Current page, 0-based. */
		page?: number;
		totalPages: number;
		/** Optional summary shown on the left (e.g. "1-25 of 213"). */
		rangeLabel?: string;
		class?: string;
	} = $props();
</script>

{#if totalPages > 1}
	<div class="flex items-center justify-between mt-3 px-1 {className}">
		<span class="text-xs text-text-tertiary">{rangeLabel ?? ''}</span>
		<div class="flex items-center gap-1">
			<Button
				size="sm"
				variant="ghost"
				disabled={page === 0}
				aria-label="Previous page"
				onclick={() => page--}
			>
				{#snippet children()}&lsaquo; Prev{/snippet}
			</Button>
			<span class="text-xs text-text-secondary tabular-nums px-2">
				{page + 1} / {totalPages}
			</span>
			<Button
				size="sm"
				variant="ghost"
				disabled={page >= totalPages - 1}
				aria-label="Next page"
				onclick={() => page++}
			>
				{#snippet children()}Next &rsaquo;{/snippet}
			</Button>
		</div>
	</div>
{/if}
