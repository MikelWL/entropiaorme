<script lang="ts">
	import type { Snippet } from 'svelte';

	// A hover/focus text popover (no click). By default the trigger is a
	// small circled "i"; pass a `trigger` snippet to hang the same popover
	// off any glyph instead. `align` picks the edge it grows from so it
	// never spills past a right-hung control.
	let {
		children,
		trigger,
		align = 'right',
		width = 'w-72',
		label = 'More information',
	}: {
		children: Snippet;
		trigger?: Snippet;
		align?: 'left' | 'right';
		width?: string;
		label?: string;
	} = $props();

	// Each instance needs its own id so the trigger describes its own popover.
	const tooltipId = `infotip-${crypto.randomUUID()}`;
</script>

<span class="group relative inline-flex items-center">
	<button
		type="button"
		aria-label={label}
		aria-describedby={tooltipId}
		class="inline-flex items-center justify-center rounded leading-none cursor-help
			transition-colors duration-[var(--duration-fast)]
			focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent/60
			{trigger
			? ''
			: 'h-4 w-4 rounded-full border border-border/70 text-[10px] font-semibold text-text-tertiary hover:text-text hover:border-border'}"
	>
		{#if trigger}{@render trigger()}{:else}i{/if}
	</button>
	<!-- Transparent bridge (pt-2) keeps hover continuous between icon and
		popover; the styled box sits inside it. -->
	<span
		id={tooltipId}
		role="tooltip"
		class="pointer-events-none absolute top-full z-30 pt-2 {width}
			opacity-0 translate-y-0.5
			transition-[opacity,transform] duration-[var(--duration-base)] ease-[var(--ease-out)]
			group-hover:opacity-100 group-hover:translate-y-0
			group-focus-within:opacity-100 group-focus-within:translate-y-0
			{align === 'right' ? 'right-0' : 'left-0'}"
	>
		<span
			class="block rounded-lg border border-border/70 bg-surface-raised p-3 text-left
				shadow-[var(--shadow-lg)]"
		>
			{@render children()}
		</span>
	</span>
</span>
