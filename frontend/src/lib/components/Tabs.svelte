<script lang="ts">
	type Tab = {
		id: string;
		label: string;
	};

	let {
		tabs,
		active,
		onchange,
		class: className = ''
	}: {
		tabs: Tab[];
		active: string;
		onchange: (id: string) => void;
		class?: string;
	} = $props();
</script>

<div class="relative flex gap-1 border-b border-border/70 {className}" role="tablist">
	{#each tabs as tab}
		<button
			role="tab"
			data-tab-id={tab.id}
			aria-selected={active === tab.id}
			class="relative px-3.5 py-2 text-sm font-medium cursor-pointer
				transition-colors duration-[var(--duration-base)] ease-[var(--ease-out)]
				focus-visible:outline-none focus-visible:text-text
				{active === tab.id
				? 'text-accent'
				: 'text-text-secondary hover:text-text'}"
			onclick={() => onchange(tab.id)}
		>
			{tab.label}
			{#if active === tab.id}
				<span
					aria-hidden="true"
					class="pointer-events-none absolute inset-x-2 -bottom-px h-0.5 rounded-full bg-accent
						[box-shadow:0_0_10px_color-mix(in_oklab,var(--color-accent)_70%,transparent)]"
				></span>
			{/if}
		</button>
	{/each}
</div>
