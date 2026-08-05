<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { HTMLAttributes } from 'svelte/elements';
	import DefinitionAuthoring from './DefinitionAuthoring.svelte';
	import type { DefinitionsModel } from './definitionsModel.svelte';
	import SessionReview from './SessionReview.svelte';
	import type { ReviewModel } from './reviewModel.svelte';

	let {
		model,
		review,
		class: className = '',
		children,
		...rest
	}: {
		model: DefinitionsModel;
		/** The review surface's state; the stage's second full-screen
		 * surface, a peer of the authoring environment rather than a
		 * layer over it. */
		review: ReviewModel;
		class?: string;
		/** The page content the two surfaces replace. */
		children: Snippet;
	} & Omit<HTMLAttributes<HTMLDivElement>, 'class' | 'children'> = $props();

	// Authoring and review are peers, and only one is ever up: both are
	// entered from the island, and each closes back to the dashboard.
	const stageOpen = $derived(model.mode !== 'closed' || review.open);

	let stageEl = $state<HTMLDivElement | null>(null);

	// A surface takes over the page's own region, so it can only be on
	// screen if that region is scrolled to its top. The hidden content is
	// clamped (below) and cannot scroll while one is open, so this runs
	// once per opening.
	$effect(() => {
		if (!stageOpen || !stageEl) return;
		stageEl.closest('main')?.scrollTo({ top: 0 });
	});
</script>

<!-- The stage is the page's region, not the window: the sidebar and the
	 titlebar stay put, and navigating away is itself the cancel. While a
	 surface is open the page content steps aside (opacity plus a slight
	 downward drift), then the surface fades into the vacated space. The
	 asymmetric transition delays sequence both directions; `inert` takes
	 the hidden content out of the tab order while it is invisible. -->
<div class="relative h-full" bind:this={stageEl}>
	<div
		class="stage-content {className}"
		class:stage-content-hidden={stageOpen}
		inert={stageOpen}
		{...rest}
	>
		{@render children()}
	</div>

	<DefinitionAuthoring {model} />
	<SessionReview model={review} />
</div>

<style>
	.stage-content {
		transition:
			opacity var(--duration-base) var(--ease-out),
			transform var(--duration-base) var(--ease-out);
		/* Returning: wait for the open surface to leave first. */
		transition-delay: 140ms;
	}
	.stage-content-hidden {
		opacity: 0;
		transform: translateY(10px) scale(0.99);
		/* Leaving: go immediately; the surface arrives after. */
		transition-delay: 0ms;
		/* Invisible content must not keep the region scrollable: the
		   surface covering it is positioned against the region's top. */
		max-height: 100%;
		overflow: hidden;
	}
	@media (prefers-reduced-motion: reduce) {
		.stage-content {
			transition: none;
		}
	}
</style>
