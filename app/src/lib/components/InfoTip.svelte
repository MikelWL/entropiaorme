<script lang="ts">
	import type { Snippet } from 'svelte';

	// A small circled "i" that reveals a text popover on hover or keyboard
	// focus (no click). The popover anchors to the icon; `align` picks the
	// edge it grows from so it never spills past a right-hung control.
	let {
		children,
		align = 'right',
		width = 'w-72',
		label = 'More information',
	}: {
		children: Snippet;
		align?: 'left' | 'right';
		width?: string;
		label?: string;
	} = $props();
</script>

<span class="group relative inline-flex items-center">
	<button
		type="button"
		aria-label={label}
		class="inline-flex h-4 w-4 items-center justify-center rounded-full border border-border/70
			text-[10px] font-semibold leading-none text-text-tertiary cursor-help
			transition-colors duration-[var(--duration-fast)]
			hover:text-text hover:border-border
			focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent/60"
	>
		i
	</button>
	<!-- Transparent bridge (pt-2) keeps hover continuous between icon and
		popover; the styled box sits inside it. -->
	<span
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
