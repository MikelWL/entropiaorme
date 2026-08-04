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
</script>

<!-- While the authoring environment is open the hosting page steps
	 aside: its content animates away first (opacity plus a slight
	 downward drift), then the surface fades into the vacated space. The
	 asymmetric transition delays sequence both directions; `inert` takes
	 the hidden content out of the tab order while it is invisible. -->
<div
	class="stage-content {className}"
	class:stage-content-hidden={authoringOpen}
	inert={authoringOpen}
	{...rest}
>
	{@render children()}
</div>

<DefinitionAuthoring {model} />

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
	}
	@media (prefers-reduced-motion: reduce) {
		.stage-content {
			transition: none;
		}
	}
</style>
