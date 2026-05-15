<script lang="ts">
	import type { Snippet } from 'svelte';

	let {
		children,
		class: className = '',
		...rest
	}: {
		children: Snippet;
		class?: string;
		[key: string]: unknown;
	} = $props();

	const baseClasses =
		'relative rounded-md border border-border/70 bg-surface/70 shadow-md backdrop-blur-[2px] before:pointer-events-none before:absolute before:inset-0 before:rounded-[inherit] before:[box-shadow:inset_0_1px_0_0_rgba(255,255,255,0.04)]';
</script>

<div class="{baseClasses} {className}" {...rest}>
	{@render children()}
</div>
