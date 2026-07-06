<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { HTMLButtonAttributes } from 'svelte/elements';

	type Variant = 'primary' | 'secondary' | 'ghost' | 'danger';
	type Size = 'sm' | 'md' | 'lg';

	let {
		variant = 'primary',
		size = 'md',
		disabled = false,
		loading = false,
		children,
		class: className = '',
		...rest
	}: HTMLButtonAttributes & {
		variant?: Variant;
		size?: Size;
		disabled?: boolean;
		loading?: boolean;
		children: Snippet;
		class?: string;
	} = $props();

	const baseClasses =
		'relative inline-flex items-center justify-center font-medium select-none cursor-pointer ' +
		'transition-all duration-[var(--duration-base)] ease-[var(--ease-out)] ' +
		'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent focus-visible:ring-offset-1 focus-visible:ring-offset-base ' +
		'disabled:opacity-40 disabled:cursor-not-allowed disabled:pointer-events-none';

	const variantClasses: Record<Variant, string> = {
		primary:
			'bg-accent text-text-inverse border border-accent ' +
			'hover:bg-accent-hover hover:border-accent-hover hover:shadow-[var(--shadow-glow)] ' +
			'active:translate-y-px',
		secondary:
			'bg-surface/70 text-text border border-border ' +
			'hover:bg-surface-hover hover:border-border-bright ' +
			'active:translate-y-px',
		ghost:
			'bg-transparent text-text-secondary border border-transparent ' +
			'hover:text-text hover:border-border-bright/60 hover:bg-surface-hover/40 ' +
			'active:translate-y-px',
		danger:
			'bg-error/10 text-error border border-error/30 ' +
			'hover:bg-error/20 hover:border-error/50 ' +
			'active:translate-y-px'
	};

	const sizeClasses: Record<Size, string> = {
		sm: 'h-7 px-2.5 text-xs rounded-sm gap-1.5',
		md: 'h-9 px-4 text-sm rounded-md gap-2',
		lg: 'h-11 px-6 text-base rounded-md gap-2.5'
	};
</script>

<button
	class="{baseClasses} {variantClasses[variant]} {sizeClasses[size]} {className}"
	disabled={disabled || loading}
	aria-busy={loading}
	{...rest}
>
	{#if loading}
		<svg
			class="animate-spin -ml-0.5 h-4 w-4"
			xmlns="http://www.w3.org/2000/svg"
			fill="none"
			viewBox="0 0 24 24"
		>
			<circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
			<path
				class="opacity-75"
				fill="currentColor"
				d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"
			/>
		</svg>
	{/if}
	{@render children()}
</button>
