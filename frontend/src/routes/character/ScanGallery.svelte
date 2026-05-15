<script lang="ts">
	import { manualSkillScanCapturePngUrl } from '$lib/api';
	import ScanGalleryPreview from './ScanGalleryPreview.svelte';

	let {
		captured,
		expected,
		dimmed = false,
	}: {
		captured: number;
		expected: number;
		dimmed?: boolean;
	} = $props();

	let previewPage = $state<number | null>(null);

	function urlFor(page: number) {
		return manualSkillScanCapturePngUrl(page);
	}
</script>

<div class="grid grid-cols-3 gap-2 sm:grid-cols-4 md:grid-cols-6" class:opacity-60={dimmed}>
	{#each Array.from({ length: expected }, (_, i) => i + 1) as page (page)}
		{#if page <= captured}
			<button
				type="button"
				class="group relative aspect-[5/6] overflow-hidden rounded border border-border bg-surface transition-colors hover:border-accent cursor-pointer"
				onclick={() => (previewPage = page)}
				aria-label="Preview page {page}"
			>
				<img
					src={urlFor(page)}
					alt="Captured page {page}"
					class="h-full w-full object-cover"
					loading="lazy"
				/>
				<span class="absolute left-1 top-1 rounded bg-black/60 px-1.5 py-0.5 text-[10px] tabular-nums text-text">
					{page}
				</span>
			</button>
		{:else}
			<div class="flex aspect-[5/6] items-center justify-center rounded border border-dashed border-border bg-surface/40 text-xs text-text-tertiary tabular-nums">
				{page}
			</div>
		{/if}
	{/each}
</div>

{#if previewPage !== null}
	<ScanGalleryPreview src={urlFor(previewPage)} page={previewPage} onClose={() => (previewPage = null)} />
{/if}
