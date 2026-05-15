<script lang="ts">
	type Option = { id: string; label: string; disabled?: boolean };
	type Size = 'sm' | 'md';

	let {
		options,
		active,
		onchange,
		size = 'sm',
		class: className = ''
	}: {
		options: Option[];
		active: string;
		onchange: (id: string) => void;
		size?: Size;
		class?: string;
	} = $props();

	const containerBase =
		'inline-flex items-center gap-0.5 rounded-md bg-surface-hover/50 p-0.5';
	const optionBase =
		'inline-flex items-center justify-center font-medium rounded transition-colors cursor-pointer disabled:cursor-not-allowed disabled:opacity-40';
	const sizeClasses: Record<Size, string> = {
		sm: 'px-3 py-1 text-xs',
		md: 'px-4 py-1.5 text-sm'
	};
	const activeCls = 'bg-surface text-text shadow-sm';
	const inactiveCls = 'text-text-tertiary hover:text-text';
</script>

<div class="{containerBase} {className}">
	{#each options as option (option.id)}
		<button
			type="button"
			class="{optionBase} {sizeClasses[size]} {option.id === active ? activeCls : inactiveCls}"
			disabled={option.disabled}
			onclick={() => onchange(option.id)}
		>
			{option.label}
		</button>
	{/each}
</div>
