<script lang="ts">
	import type { Snippet } from 'svelte';

	type Variant = 'accent' | 'positive' | 'negative' | 'warning' | 'neutral';

	let {
		variant = 'neutral',
		children,
		class: className = ''
	}: {
		variant?: Variant;
		children: Snippet;
		class?: string;
	} = $props();

	const variantClasses: Record<Variant, string> = {
		accent: 'bg-accent-muted/30 text-accent border-accent/30',
		positive: 'bg-positive-muted/30 text-positive border-positive/30',
		negative: 'bg-negative-muted/30 text-negative border-negative/30',
		warning: 'bg-warning/10 text-warning border-warning/30',
		neutral: 'bg-surface-hover/70 text-text-secondary border-border'
	};
</script>

<span
	class="inline-flex items-center rounded-sm border px-1.5 py-0.5 text-[10.5px] font-medium tracking-wide tabular-nums {variantClasses[variant]} {className}"
>
	{@render children()}
</span>
