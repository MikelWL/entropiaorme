<script lang="ts">
	import type { Snippet } from 'svelte';

	type Trend = 'up' | 'down' | 'neutral';

	let {
		label,
		value,
		unit = '',
		trend,
		comparison,
		valueClass = '',
		class: className = '',
		labelSuffix,
		emphasis = 'primary',
	}: {
		label: string;
		value: string | number;
		unit?: string;
		trend?: Trend;
		comparison?: string;
		/** Extra classes on the value text (e.g. a positive/negative tone
		 * for a net figure). Defaults to the neutral text colour. */
		valueClass?: string;
		class?: string;
		labelSuffix?: Snippet;
		emphasis?: 'primary' | 'secondary';
	} = $props();

	const trendColors: Record<Trend, string> = {
		up: 'text-positive',
		down: 'text-negative',
		neutral: 'text-text-secondary'
	};

	const trendIcons: Record<Trend, string> = {
		up: '↑',
		down: '↓',
		neutral: '→'
	};
</script>

<div class="flex flex-col gap-1.5 {className}">
	<div class="flex items-center gap-1.5">
		<span class="eyebrow">{label}</span>
		{#if labelSuffix}{@render labelSuffix()}{/if}
	</div>
	<div class="flex h-6 items-end">
		<div class="flex items-baseline gap-1.5">
			<span
				class="tabular-nums leading-none
					{emphasis === 'primary' ? 'text-2xl font-semibold' : 'text-base font-medium'}
					{valueClass || (emphasis === 'primary' ? 'text-text' : 'text-text-secondary')}"
			>
				{value}
			</span>
			{#if unit}
				<span
					class="font-medium uppercase tracking-wider text-text-tertiary
						{emphasis === 'primary' ? 'text-xs' : 'text-[10px]'}"
				>
					{unit}
				</span>
			{/if}
		</div>
	</div>
	{#if trend || comparison}
		<div class="flex items-center gap-1.5 text-xs tabular-nums">
			{#if trend}
				<span class="{trendColors[trend]} font-medium">
					{trendIcons[trend]}
				</span>
			{/if}
			{#if comparison}
				<span class="text-text-tertiary">{comparison}</span>
			{/if}
		</div>
	{/if}
</div>
