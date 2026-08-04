<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { HTMLAttributes } from 'svelte/elements';
	import DefinitionAuthoring from './DefinitionAuthoring.svelte';
	import type { DefinitionsModel } from './definitionsModel.svelte';

	let {
		model,
		class: className = '',
		children,
		...rest
	}: {
		model: DefinitionsModel;
		class?: string;
		/** The page content the authoring environment replaces. */
		children: Snippet;
	} & Omit<HTMLAttributes<HTMLDivElement>, 'class' | 'children'> = $props();

	const authoringOpen = $derived(model.mode !== 'closed');

	let stageEl = $state<HTMLDivElement | null>(null);

	// The environment takes over the page's own region, so it can only be
	// on screen if that region is scrolled to its top. The hidden content
	// is clamped (below) and cannot scroll while it is open, so this runs
	// once per opening.
	$effect(() => {
		if (!authoringOpen || !stageEl) return;
		stageEl.closest('main')?.scrollTo({ top: 0 });
	});
</script>

<!-- The stage is the page's region, not the window: the sidebar and the
	 titlebar stay put, and navigating away is itself the cancel. While the
	 authoring environment is open the page content steps aside (opacity
	 plus a slight downward drift), then the surface fades into the vacated
	 space. The asymmetric transition delays sequence both directions;
	 `inert` takes the hidden content out of the tab order while it is
	 invisible. -->
<div class="relative h-full" bind:this={stageEl}>
	<div
		class="stage-content {className}"
		class:stage-content-hidden={authoringOpen}
		inert={authoringOpen}
		{...rest}
	>
		{@render children()}
	</div>

	<DefinitionAuthoring {model} />
</div>

<style>
	.stage-content {
		transition:
			opacity var(--duration-base) var(--ease-out),
			transform var(--duration-base) var(--ease-out);
		/* Returning: wait for the authoring surface to leave first. */
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
