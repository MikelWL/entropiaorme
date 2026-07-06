<script lang="ts">
	import { onMount } from 'svelte';
	import { getPreference } from '$lib/preferences';
	import { closeGuide, openGuide } from '$lib/guide/engine';
	import { guideState } from '$lib/guide/state.svelte';
	import { characterSurface } from '$lib/guide/surfaces/character';
	import CharacterView from './CharacterView.svelte';

	let guideSeen = $state(true);
	function toggleSurfaceGuide(): void {
		if (guideState.isActive) {
			closeGuide();
		} else {
			guideSeen = true;
			void openGuide(characterSurface);
		}
	}

	onMount(() => {
		void (async () => {
			guideSeen = await getPreference<boolean>('guide_seen_character', false);
		})();
	});
</script>

<div class="px-6 pb-6 space-y-6">
	<div class="flex items-center justify-between">
		<header class="flex flex-col gap-1.5">
			<h1 class="text-xl font-semibold text-text tracking-tight">Character</h1>
			<span class="block h-px w-12 bg-gradient-to-r from-accent/60 to-transparent"></span>
			<p class="text-sm text-text-secondary mt-0.5">Skills, attributes, professions, and codex progress</p>
		</header>
		<div class="flex items-center gap-2">
			<button
				type="button"
				onclick={toggleSurfaceGuide}
				title={guideState.isActive ? 'Exit guide' : 'Open guide'}
				aria-label={guideState.isActive ? 'Exit guide' : 'Open guide for this page'}
				class="relative h-8 w-8 rounded-full border border-border bg-surface hover:bg-surface-hover text-text-secondary hover:text-text transition-colors flex items-center justify-center text-sm font-semibold {guideState.isActive ? 'z-[9100]' : ''}"
			>
				{#if guideState.isActive}
					<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="currentColor" class="w-3.5 h-3.5" aria-hidden="true">
						<path d="M5.28 4.22a.75.75 0 00-1.06 1.06L6.94 8l-2.72 2.72a.75.75 0 101.06 1.06L8 9.06l2.72 2.72a.75.75 0 101.06-1.06L9.06 8l2.72-2.72a.75.75 0 00-1.06-1.06L8 6.94 5.28 4.22z" />
					</svg>
				{:else}
					?
				{/if}
				{#if !guideSeen}
					<span class="absolute -top-0.5 -right-0.5 h-2 w-2 rounded-full bg-accent"></span>
				{/if}
			</button>
		</div>
	</div>

	<CharacterView />
</div>
