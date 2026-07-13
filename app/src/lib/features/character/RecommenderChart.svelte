<script lang="ts">
	import type { RecommenderActivity } from '$lib/api/commands.gen';

	let {
		selected,
		direct,
		pesCap,
		sampleStep,
		unitLabel,
	}: {
		/** The chosen candidate: the primary (arbitrage) line. */
		selected: RecommenderActivity;
		/** The target's own activity: the faded reference line. */
		direct: RecommenderActivity | null;
		pesCap: number;
		sampleStep: number;
		/** Y-axis unit: profession levels or HP. */
		unitLabel: string;
	} = $props();

	const CHART_HEIGHT = 240;
	const PAD_L = 44; // room for y-axis labels
	const PAD_R = 12;
	const PAD_T = 12;
	const PAD_B = 26; // room for x-axis labels
	const PLOT_H = CHART_HEIGHT - PAD_T - PAD_B;
	const FALLBACK_WIDTH = 600; // until the bind:clientWidth lands

	let chartWidth = $state(FALLBACK_WIDTH);
	let plotWidth = $derived(chartWidth - PAD_L - PAD_R);

	// One shared scale across both lines so the comparison is honest;
	// the +1 baseline always stays in view.
	const yMax = $derived(
		Math.max(1, ...selected.series, ...(direct?.series ?? [])) * 1.08,
	);

	function xAt(index: number): number {
		return PAD_L + ((index * sampleStep) / pesCap) * plotWidth;
	}

	function yAt(value: number): number {
		return PAD_T + PLOT_H - (Math.min(value, yMax) / yMax) * PLOT_H;
	}

	function pathOf(series: number[]): string {
		return series
			.map((value, i) => `${i === 0 ? 'M' : 'L'}${xAt(i).toFixed(1)},${yAt(value).toFixed(1)}`)
			.join(' ');
	}

	const selectedPath = $derived(pathOf(selected.series));
	const directPath = $derived(direct ? pathOf(direct.series) : '');
	const plotBottom = PAD_T + PLOT_H;
	const xTicks = $derived([0, 0.25, 0.5, 0.75, 1].map((f) => f * pesCap));
	const yTicks = $derived([0.5, 1].map((f) => f * yMax));
</script>

<div class="w-full" bind:clientWidth={chartWidth}>
	<svg
		class="block w-full"
		height={CHART_HEIGHT}
		viewBox="0 0 {chartWidth} {CHART_HEIGHT}"
		role="img"
		aria-label="Projected {unitLabel} gained by skilling budget: {selected.activity}{direct
			? ` compared with ${direct.activity}`
			: ''}"
	>
		<!-- Axes -->
		<line
			x1={PAD_L}
			y1={plotBottom}
			x2={chartWidth - PAD_R}
			y2={plotBottom}
			stroke="var(--color-border)"
		/>
		<line x1={PAD_L} y1={PAD_T} x2={PAD_L} y2={plotBottom} stroke="var(--color-border)" />
		{#each xTicks as tick}
			<text
				x={PAD_L + (tick / pesCap) * plotWidth}
				y={plotBottom + 16}
				text-anchor="middle"
				class="fill-text-tertiary text-[10px] tabular-nums"
			>
				{tick}
			</text>
		{/each}
		{#each yTicks as tick}
			<text
				x={PAD_L - 6}
				y={yAt(tick) + 3}
				text-anchor="end"
				class="fill-text-tertiary text-[10px] tabular-nums"
			>
				{tick >= 10 ? tick.toFixed(0) : tick.toFixed(1)}
			</text>
		{/each}

		<!-- +1 baseline: the ranking metric made visible -->
		<line
			x1={PAD_L}
			y1={yAt(1)}
			x2={chartWidth - PAD_R}
			y2={yAt(1)}
			stroke="var(--color-text-tertiary)"
			stroke-dasharray="3 4"
			opacity="0.6"
		/>
		<text
			x={chartWidth - PAD_R}
			y={yAt(1) - 4}
			text-anchor="end"
			class="fill-text-tertiary text-[10px]"
		>
			+1
		</text>

		<!-- Direct-grind reference: deliberately faded, never co-equal
		     (for some targets no real grind path exists). -->
		{#if direct}
			<path d={directPath} fill="none" stroke="var(--color-text-tertiary)" stroke-width="1.5" opacity="0.35" />
		{/if}

		<!-- The selected candidate -->
		<path d={selectedPath} fill="none" stroke="var(--color-accent)" stroke-width="2" />
	</svg>

	<div class="mt-1 flex items-center justify-between gap-4 text-xs text-text-tertiary">
		<div class="flex items-center gap-4">
			<span class="flex items-center gap-1.5">
				<span class="inline-block h-0.5 w-4 rounded bg-accent"></span>
				<span class="text-text-secondary">{selected.activity}</span>
			</span>
			{#if direct}
				<span class="flex items-center gap-1.5">
					<span class="inline-block h-0.5 w-4 rounded bg-text-tertiary opacity-40"></span>
					<span>{direct.activity} (reference)</span>
				</span>
			{/if}
		</div>
		<span>PES poured in &rarr; {unitLabel} gained</span>
	</div>
</div>
