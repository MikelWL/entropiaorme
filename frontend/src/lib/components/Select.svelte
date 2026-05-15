<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { HTMLSelectAttributes } from 'svelte/elements';

	let {
		value = $bindable<string | number | null>(''),
		class: className = '',
		children,
		...rest
	}: {
		value?: string | number | null;
		class?: string;
		children: Snippet;
	} & Omit<HTMLSelectAttributes, 'value' | 'class'> = $props();

	const baseClasses =
		'w-full h-9 pl-3 pr-9 text-sm bg-surface/70 text-text rounded-md border border-border appearance-none cursor-pointer transition-[border-color,box-shadow,background-color] duration-[var(--duration-base)] ease-[var(--ease-out)] hover:border-border-bright focus:outline-none focus:bg-surface focus:border-accent/60 focus:[box-shadow:var(--shadow-glow)] disabled:opacity-50 disabled:cursor-not-allowed disabled:hover:border-border';
</script>

<div class="relative {className}">
	<select bind:value class={baseClasses} {...rest}>
		{@render children()}
	</select>

	<svg
		class="absolute right-3 top-1/2 -translate-y-1/2 h-4 w-4 text-text-tertiary pointer-events-none"
		xmlns="http://www.w3.org/2000/svg"
		viewBox="0 0 20 20"
		fill="currentColor"
		aria-hidden="true"
	>
		<path
			fill-rule="evenodd"
			d="M5.23 7.21a.75.75 0 011.06.02L10 11.06l3.71-3.83a.75.75 0 111.08 1.04l-4.25 4.39a.75.75 0 01-1.08 0L5.21 8.27a.75.75 0 01.02-1.06z"
			clip-rule="evenodd"
		/>
	</svg>
</div>
