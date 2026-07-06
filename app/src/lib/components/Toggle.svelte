<script lang="ts">
	interface Props {
		checked: boolean;
		onchange?: (checked: boolean) => void;
		disabled?: boolean;
		label?: string;
		title?: string;
	}

	let { checked, onchange, disabled = false, label = '', title }: Props = $props();
</script>

<button
	class="relative w-10 h-5 rounded-full shrink-0 cursor-pointer
		transition-[background-color,box-shadow] duration-[var(--duration-base)] ease-[var(--ease-out)]
		focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent focus-visible:ring-offset-2 focus-visible:ring-offset-base
		{disabled
		? 'bg-surface-hover/40 cursor-not-allowed'
		: checked
			? 'bg-accent shadow-[var(--shadow-glow)]'
			: 'bg-border-bright hover:bg-border-bright/80'}"
	{disabled}
	onclick={() => onchange?.(!checked)}
	{title}
	role="switch"
	aria-checked={checked}
	aria-label={label}
>
	<div
		class="absolute top-0.5 w-4 h-4 rounded-full shadow-sm
			transition-transform duration-[var(--duration-base)] ease-[var(--ease-out)]
			{checked
			? 'translate-x-5.5 bg-text-inverse'
			: 'translate-x-0.5 bg-text-tertiary'}"
	></div>
</button>
