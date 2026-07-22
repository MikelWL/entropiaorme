<script lang="ts">
	type Trend = 'up' | 'down' | 'neutral';

	let {
		label,
		value,
		unit = '',
		trend,
		comparison,
		valueClass = '',
		class: className = ''
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
	<span class="eyebrow">
		{label}
	</span>
	<div class="flex items-baseline gap-1.5">
		<span class="text-2xl font-semibold tabular-nums leading-none {valueClass || 'text-text'}">
			{value}
		</span>
		{#if unit}
			<span class="text-xs font-medium text-text-tertiary uppercase tracking-wider">{unit}</span>
		{/if}
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
