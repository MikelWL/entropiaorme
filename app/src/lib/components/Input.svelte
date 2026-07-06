<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { HTMLInputAttributes } from 'svelte/elements';

	let {
		value = $bindable<string | number | null>(''),
		class: className = '',
		prefix,
		suffix,
		...rest
	}: {
		value?: string | number | null;
		class?: string;
		prefix?: Snippet;
		suffix?: Snippet;
	} & Omit<HTMLInputAttributes, 'value' | 'class'> = $props();

	const baseClasses =
		'w-full h-9 text-sm bg-surface/70 text-text rounded-md border border-border tabular-nums placeholder:text-text-tertiary transition-[border-color,box-shadow,background-color] duration-[var(--duration-base)] ease-[var(--ease-out)] hover:border-border-bright focus:outline-none focus:bg-surface focus:border-accent/60 focus:[box-shadow:var(--shadow-glow)] disabled:opacity-50 disabled:cursor-not-allowed disabled:hover:border-border';

	let paddingClasses = $derived(
		prefix && suffix ? 'pl-9 pr-9' : prefix ? 'pl-9 pr-3' : suffix ? 'pl-3 pr-9' : 'px-3'
	);
</script>

<div class="relative {className}">
	{#if prefix}
		<div
			class="absolute left-3 top-1/2 -translate-y-1/2 text-text-tertiary pointer-events-none flex items-center"
		>
			{@render prefix()}
		</div>
	{/if}

	<input bind:value class="{baseClasses} {paddingClasses}" {...rest} />

	{#if suffix}
		<div
			class="absolute right-3 top-1/2 -translate-y-1/2 text-text-tertiary flex items-center"
		>
			{@render suffix()}
		</div>
	{/if}
</div>
