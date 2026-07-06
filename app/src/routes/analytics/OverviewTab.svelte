<script lang="ts">
	import { getAnalyticsOverview } from '$lib/api';
	import type { OverviewStats, TimelineDay, MonthlyEntry } from '$lib/types/analytics';
	import { formatPed, formatPercent, formatDate } from '$lib/utils/format';
	import Card from '$lib/components/Card.svelte';
	import Divider from '$lib/components/Divider.svelte';
	import SegmentedControl from '$lib/components/SegmentedControl.svelte';

	let data = $state<OverviewStats | null>(null);
	let loading = $state(true);
	let error = $state<string | null>(null);

	// --- Return config: per-source toggles ---
	// lootTt (gains) and trackingCost (losses) are always on — not in config.
	const PROGRESSION_GAIN_TAGS = new Set(['codex']);

	interface ReturnConfig {
		gainTags: Record<string, boolean>;
		lossTags: Record<string, boolean>;
	}

	let config = $state<ReturnConfig>({ gainTags: {}, lossTags: {} });

	function initConfig(stats: OverviewStats) {
		const gainTags: Record<string, boolean> = {};
		for (const tag of Object.keys(stats.returnsBreakdown.ledger)) {
			if (PROGRESSION_GAIN_TAGS.has(tag)) continue;
			gainTags[tag] = true;
		}
		const lossTags: Record<string, boolean> = {};
		for (const tag of Object.keys(stats.lossesBreakdown.ledger)) {
			lossTags[tag] = true;
		}
		config = { gainTags, lossTags };
	}

	let activeRange = $state('All Time');
	let showBreakdown = $state(false);
	const ranges = ['All Time', '30d', '90d', '1y'];
	const periodMap: Record<string, string> = {
		'All Time': 'all',
		'30d': '30d',
		'90d': '90d',
		'1y': '1y'
	};

	$effect(() => {
		const period = periodMap[activeRange];
		loadData(period);
	});

	async function loadData(period: string) {
		loading = true;
		error = null;
		try {
			data = await getAnalyticsOverview(period);
			initConfig(data);
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load overview';
		} finally {
			loading = false;
		}
	}

	// --- Config-aware aggregation helpers ---
	function sumRecord(rec: Record<string, number>, enabled: Record<string, boolean>): number {
		let total = 0;
		for (const [tag, amount] of Object.entries(rec)) {
			if (enabled[tag]) total += amount;
		}
		return total;
	}

	function dayGains(d: TimelineDay): number {
		let total = d.lootTt;
		total += sumRecord(d.ledgerGains, config.gainTags);
		return total;
	}
	function dayLosses(d: TimelineDay): number {
		return d.trackingCost + sumRecord(d.ledgerLosses, config.lossTags);
	}
	function monthGains(m: MonthlyEntry): number {
		let total = m.lootTt;
		total += sumRecord(m.ledgerGains, config.gainTags);
		return total;
	}
	function monthLosses(m: MonthlyEntry): number {
		return m.trackingCost + sumRecord(m.ledgerLosses, config.lossTags);
	}

	// --- Donut chart ---
	const PIE_R = 50;
	const PIE_C = 2 * Math.PI * PIE_R;
	let hoveredIdx = $state(-1);

	const segmentColors: Record<string, string> = {
		lootTt: '#38bdf8',
		item_sale: '#fbbf24',
		quest_reward: '#a78bfa',
		inventory_sale: '#f472b6',
		other: '#fb7185'
	};

	const tagLabels: Record<string, string> = {
		lootTt: 'TT Loot',
		item_sale: 'Auction Sales',
		quest_reward: 'Quest Rewards',
		inventory_sale: 'Mayhem',
		repair: 'Repairs',
		equipment: 'Equipment',
		other: 'Other'
	};

	function labelFor(key: string): string {
		return tagLabels[key] || key.charAt(0).toUpperCase() + key.slice(1).replace(/_/g, ' ');
	}

	function colorFor(key: string): string {
		return segmentColors[key] || '#94a3b8';
	}

	interface PieView {
		rate: number;
		gains: number;
		losses: number;
		arcs: { label: string; ped: number; pct: number; color: string; length: number; offset: number }[];
	}

	let pieView = $derived.by((): PieView | null => {
		if (!data) return null;

		const rb = data.returnsBreakdown;

		// Build gain sources from config
		const sources: { key: string; ped: number }[] = [];
		if (rb.lootTt > 0) sources.push({ key: 'lootTt', ped: rb.lootTt });
		for (const [tag, amount] of Object.entries(rb.ledger)) {
			if (config.gainTags[tag] && amount > 0) sources.push({ key: tag, ped: amount });
		}

		const gains = sources.reduce((sum, s) => sum + s.ped, 0);

		// Build losses from config
		let losses = data.lossesBreakdown.trackingCost;
		for (const [tag, amount] of Object.entries(data.lossesBreakdown.ledger)) {
			if (config.lossTags[tag]) losses += amount;
		}

		if (losses <= 0 || gains <= 0) return null;

		const arcs: PieView['arcs'] = [];
		let offset = 0;
		for (const { key, ped } of sources) {
			const length = (ped / gains) * PIE_C;
			arcs.push({
				label: labelFor(key), ped,
				pct: ped / losses, color: colorFor(key),
				length, offset
			});
			offset += length;
		}
		return { rate: gains / losses, gains, losses, arcs };
	});

	// --- Timeline (config-aware cumulative P&L) ---
	let chartPoints = $derived.by(() => {
		if (!data || data.timeline.length < 2) return [];
		let cumulative = 0;
		const vals = data.timeline.map((d) => {
			cumulative += dayGains(d) - dayLosses(d);
			return { date: d.date, net: cumulative };
		});
		const nets = vals.map((v) => v.net);
		const minV = Math.min(...nets, 0);
		const maxV = Math.max(...nets, 0);
		const range = maxV - minV || 1;
		// Y-mapping: the data line is bounded between y=28 (top) and y=140 (bottom).
		// Top reserves 18px of headroom so the end-of-period current-net label
		// (which sits above the rightmost dot) never overlaps the line, even when
		// the line peaks at the chart's all-time-high right edge.
		return vals.map((v, i) => ({
			x: 40 + (i / (vals.length - 1)) * 720,
			y: 28 + ((maxV - v.net) / range) * 112,
			value: Math.round(v.net * 100) / 100,
			date: v.date
		}));
	});

	let chartPath = $derived(chartPoints.map((p) => `${p.x},${p.y}`).join(' '));

	// Fill polygon closes at the zero line (not the bottom of the chart) so the
	// above-zero half lives between the data line and zeroY, the below-zero half
	// likewise. Each half is then clipped + tinted to its sign-coloured gradient.
	let chartFillPath = $derived.by(() => {
		if (chartPoints.length < 2) return '';
		const last = chartPoints[chartPoints.length - 1];
		const first = chartPoints[0];
		return chartPath + ` ${last.x},${zeroY} ${first.x},${zeroY}`;
	});

	let zeroY = $derived.by(() => {
		if (chartPoints.length < 2) return 84;
		const vals = chartPoints.map((p) => p.value);
		const minV = Math.min(...vals, 0);
		const maxV = Math.max(...vals, 0);
		const range = maxV - minV || 1;
		return 28 + ((maxV - 0) / range) * 112;
	});

	// --- Monthly (config-aware) ---
	let monthlyRows = $derived.by(() => {
		if (!data) return [];
		return data.monthlyBreakdown.map((m) => {
			const gains = monthGains(m);
			const losses = monthLosses(m);
			const net = gains - losses;
			const globalRate = losses > 0 ? gains / losses : null;
			const cycled = m.trackingCost;
			const lootRate = cycled > 0 ? m.lootTt / cycled : null;
			return {
				month: m.month,
				cost: losses,
				returns: gains,
				net,
				lootRate,
				globalRate,
				pes: m.pes + m.codexPes + m.questPes
			};
		});
	});

</script>

{#if loading}
	<p class="text-sm text-text-secondary">Loading overview...</p>
{:else if error}
	<p class="text-sm text-error">{error}</p>
{:else if data}
	<div class="space-y-6" data-guide-anchor="analytics-overview-area">

		<!-- Returns breakdown: donut + legend | gains/losses -->
		{#if pieView}
			<div>
				<div class="flex items-center justify-between gap-4 mb-3">
					<h3 class="eyebrow">
						Global Returns
					</h3>

					<SegmentedControl
						class="flex-shrink-0"
						options={ranges.map((r) => ({ id: r, label: r }))}
						active={activeRange}
						onchange={(id) => (activeRange = id)}
					/>
				</div>
				<div class="flex flex-col gap-2 min-w-0 mb-4">
					<div class="flex items-center gap-1.5 flex-wrap">
						<span class="text-[10px] font-semibold uppercase tracking-wider text-text-tertiary w-12 flex-shrink-0">
							Returns
						</span>
						<span
							class="px-2.5 py-1 text-xs font-medium rounded-md bg-accent/15 text-accent cursor-default"
							title="Always included"
						>
							TT Loot
						</span>
						{#each Object.keys(config.gainTags) as tag}
							<button
								type="button"
								class="filter-chip {config.gainTags[tag] ? 'is-active' : ''}"
								onclick={() => (config.gainTags[tag] = !config.gainTags[tag])}
							>
								{labelFor(tag)}
							</button>
						{/each}
					</div>
					<div class="flex items-center gap-1.5 flex-wrap">
						<span class="text-[10px] font-semibold uppercase tracking-wider text-text-tertiary w-12 flex-shrink-0">
							Costs
						</span>
						<span
							class="px-2.5 py-1 text-xs font-medium rounded-md bg-accent/15 text-accent cursor-default"
							title="Always included: weapon, healing, enhancers, armour"
						>
							Cycled
						</span>
						{#each Object.keys(config.lossTags) as tag}
							<button
								type="button"
								class="filter-chip {config.lossTags[tag] ? 'is-active' : ''}"
								onclick={() => (config.lossTags[tag] = !config.lossTags[tag])}
							>
								{labelFor(tag)}
							</button>
						{/each}
					</div>
				</div>
				<div class="flex items-center justify-center gap-12">
				<!-- Donut chart + legend -->
				<div class="flex flex-col items-center gap-3 flex-shrink-0">
					<div class="relative">
						<svg
							role="img"
							aria-label="Cost breakdown donut chart"
							viewBox="0 0 120 120"
							class="w-40 h-40"
							onmousemove={(e) => {
								const svg = e.currentTarget;
								const rect = svg.getBoundingClientRect();
								const x = ((e.clientX - rect.left) / rect.width) * 120 - 60;
								const y = ((e.clientY - rect.top) / rect.height) * 120 - 60;
								const dist = Math.sqrt(x * x + y * y);
								if (dist < PIE_R - 7 || dist > PIE_R + 7) { hoveredIdx = -1; return; }
								let angle = Math.atan2(y, x) + Math.PI / 2;
								if (angle < 0) angle += Math.PI * 2;
								const pos = (angle / (Math.PI * 2)) * PIE_C;
								if (!pieView) { hoveredIdx = -1; return; }
								for (let i = 0; i < pieView.arcs.length; i++) {
									const a = pieView.arcs[i];
									if (pos >= a.offset && pos < a.offset + a.length) { hoveredIdx = i; return; }
								}
								hoveredIdx = -1;
							}}
							onmouseleave={() => (hoveredIdx = -1)}
						>
							{#each pieView.arcs as arc, i}
								<circle
									cx="60"
									cy="60"
									r={PIE_R}
									fill="none"
									stroke={arc.color}
									stroke-width={hoveredIdx === i ? 13 : 10}
									stroke-opacity={hoveredIdx >= 0 && hoveredIdx !== i ? 0.35 : 1}
									stroke-dasharray="{arc.length} {PIE_C - arc.length}"
									stroke-dashoffset={-arc.offset}
									transform="rotate(-90 60 60)"
									class="transition-all duration-150"
								/>
							{/each}
						</svg>
						<!-- Center label -->
						<div class="absolute inset-0 flex flex-col items-center justify-center pointer-events-none">
							{#if hoveredIdx >= 0 && pieView.arcs[hoveredIdx]}
								{@const seg = pieView.arcs[hoveredIdx]}
								<span class="text-[10px] font-medium text-text-secondary">{seg.label}</span>
								<span class="text-base font-bold tabular-nums text-text">
									{formatPed(seg.ped)}
								</span>
								<span class="text-[10px] tabular-nums text-text-tertiary">
									{formatPercent(seg.pct)} of costs
								</span>
							{:else}
								<span class="text-xl font-bold tabular-nums text-text">
									{formatPercent(pieView.rate)}
								</span>
								<span class="text-[10px] text-text-tertiary">
									return rate
								</span>
							{/if}
						</div>
					</div>
					<!-- Colour legend -->
					<div class="flex flex-wrap justify-center gap-x-4 gap-y-1">
						{#each pieView.arcs as seg, i}
							<button
								class="flex items-center gap-1.5 text-xs cursor-pointer transition-opacity duration-150
									{hoveredIdx >= 0 && hoveredIdx !== i ? 'opacity-40' : 'opacity-100'}"
								onmouseenter={() => (hoveredIdx = i)}
								onmouseleave={() => (hoveredIdx = -1)}
							>
								<span
									class="w-2 h-2 rounded-full flex-shrink-0"
									style="background: {seg.color}"
								></span>
								<span class="text-text-secondary">{seg.label}</span>
							</button>
						{/each}
					</div>
				</div>

				<!-- Returns / Costs -->
				<div class="flex flex-col gap-4 min-w-0">
					<div class="flex flex-col gap-0.5">
						<span class="text-xs text-text-tertiary font-medium uppercase tracking-wide">
							Returns
						</span>
						<span class="text-2xl font-bold tabular-nums text-text">
							{formatPed(pieView.gains)}
							<span class="text-sm font-normal text-text-tertiary">PED</span>
						</span>
					</div>
					<div class="flex flex-col gap-0.5">
						<span class="text-xs text-text-tertiary font-medium uppercase tracking-wide">
							Costs
						</span>
						<span class="text-2xl font-bold tabular-nums text-text">
							{formatPed(pieView.losses)}
							<span class="text-sm font-normal text-text-tertiary">PED</span>
						</span>
					</div>
					<div>
						<span
							class="text-lg font-semibold tabular-nums {pieView.gains - pieView.losses >= 0
								? 'text-positive'
								: 'text-negative'}"
						>
							{pieView.gains - pieView.losses >= 0 ? '+' : ''}{formatPed(pieView.gains - pieView.losses)} PED
						</span>
					</div>
				</div>
			</div>
			</div>

			<Divider />
		{/if}

		<!-- Cumulative P&L timeline -->
		<div>
			<h3 class="eyebrow mb-3">
				Cumulative P&L
			</h3>
			{#if chartPoints.length >= 2}
				<Card class="p-4">
					<div class="h-44">
						<svg viewBox="0 0 800 160" class="w-full h-full" preserveAspectRatio="xMidYMid meet">
							<defs>
								<linearGradient id="plGradientPositive" x1="0" y1="0" x2="0" y2="1">
									<stop offset="0%" stop-color="var(--color-positive)" stop-opacity="0.18" />
									<stop offset="100%" stop-color="var(--color-positive)" stop-opacity="0.02" />
								</linearGradient>
								<linearGradient id="plGradientNegative" x1="0" y1="1" x2="0" y2="0">
									<stop offset="0%" stop-color="var(--color-negative)" stop-opacity="0.18" />
									<stop offset="100%" stop-color="var(--color-negative)" stop-opacity="0.02" />
								</linearGradient>
								<clipPath id="plClipAboveZero">
									<rect x="0" y="0" width="800" height={zeroY} />
								</clipPath>
								<clipPath id="plClipBelowZero">
									<rect x="0" y={zeroY} width="800" height={160 - zeroY} />
								</clipPath>
							</defs>

							<!-- Zero line -->
							<line
								x1="40"
								y1={zeroY}
								x2="760"
								y2={zeroY}
								stroke="var(--color-border-bright)"
								stroke-width="1"
								stroke-dasharray="4 4"
							/>
							<text x="4" y={zeroY + 4} fill="var(--color-text-tertiary)" font-size="10">0</text>

							<!-- Fill area: above-zero in green, below-zero in orange. Single
							     polygon closing at the zero line; rendered twice with
							     opposite clipPaths + opposite gradients. -->
							{#if chartFillPath}
								<polygon
									points={chartFillPath}
									fill="url(#plGradientPositive)"
									clip-path="url(#plClipAboveZero)"
								/>
								<polygon
									points={chartFillPath}
									fill="url(#plGradientNegative)"
									clip-path="url(#plClipBelowZero)"
								/>
							{/if}

							<!-- Line: same trick — render once green clipped above, once orange clipped below. -->
							{#if chartPath}
								<polyline
									points={chartPath}
									fill="none"
									stroke="var(--color-positive)"
									stroke-width="2"
									stroke-linejoin="round"
									stroke-linecap="round"
									clip-path="url(#plClipAboveZero)"
								/>
								<polyline
									points={chartPath}
									fill="none"
									stroke="var(--color-negative)"
									stroke-width="2"
									stroke-linejoin="round"
									stroke-linecap="round"
									clip-path="url(#plClipBelowZero)"
								/>
							{/if}

							<!-- Data points: per-point colour by sign. -->
							{#each chartPoints as point}
								<circle
									cx={point.x}
									cy={point.y}
									r="3"
									fill={point.value >= 0 ? 'var(--color-positive)' : 'var(--color-negative)'}
								/>
							{/each}

							<!-- End value label: 12px above last dot (with the y-mapping
							     reserving 18px headroom at the chart top). Colour tracks
							     the sign of the current net. -->
							{#if chartPoints.length > 0}
								{@const last = chartPoints[chartPoints.length - 1]}
								<text
									x={last.x}
									y={last.y - 12}
									fill={last.value >= 0 ? 'var(--color-positive)' : 'var(--color-negative)'}
									font-size="11"
									font-weight="600"
									text-anchor="end"
								>
									{formatPed(last.value)} PED
								</text>
							{/if}

							<!-- Date labels -->
							{#if chartPoints.length > 0}
								<text x="40" y="155" fill="var(--color-text-tertiary)" font-size="10">
									{formatDate(chartPoints[0].date)}
								</text>
								<text
									x="760"
									y="155"
									fill="var(--color-text-tertiary)"
									font-size="10"
									text-anchor="end"
								>
									{formatDate(chartPoints[chartPoints.length - 1].date)}
								</text>
							{/if}
						</svg>
					</div>
				</Card>
			{:else}
				<Card class="p-6">
					<p class="text-sm text-text-tertiary text-center">
						Not enough data for a timeline. Complete more sessions to see your P&L trend.
					</p>
				</Card>
			{/if}
		</div>

		<!-- Cumulative breakdown table (collapsed by default) -->
		{#if data}
			{@const lb = data.lossesBreakdown}
			{@const rb = data.returnsBreakdown}
			{@const cb = lb.cycledBreakdown}
			{@const totalLedgerGains = Object.entries(rb.ledger).reduce((s, [tag, v]) => s + (PROGRESSION_GAIN_TAGS.has(tag) ? 0 : v), 0)}
			{@const totalLedgerLosses = Object.values(lb.ledger).reduce((s, v) => s + v, 0)}
			{@const totalReturns = rb.lootTt + totalLedgerGains}
			{@const totalCosts = lb.trackingCost + totalLedgerLosses}
			<div class="mt-2">
				<button
					class="flex items-center gap-1.5 eyebrow mb-3 cursor-pointer hover:text-text transition-colors"
					onclick={() => (showBreakdown = !showBreakdown)}
				>
					<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor"
						class="h-3.5 w-3.5 transition-transform duration-150 {showBreakdown ? 'rotate-180' : ''}">
						<path fill-rule="evenodd" d="M5.23 7.21a.75.75 0 011.06.02L10 11.168l3.71-3.938a.75.75 0 111.08 1.04l-4.25 4.5a.75.75 0 01-1.08 0l-4.25-4.5a.75.75 0 01.02-1.06z" clip-rule="evenodd" />
					</svg>
					Cumulative Breakdown
				</button>
				{#if showBreakdown}
				<Card class="p-4">
					<table class="w-full text-sm">
						<tbody>
							<!-- Returns section -->
							<tr class="border-b border-border">
								<td class="py-2 text-text font-medium">Returns</td>
								<td class="py-2 text-right tabular-nums text-text font-medium">{formatPed(totalReturns)}</td>
							</tr>
							<tr class="border-b border-border/30">
								<td class="py-1.5 pl-5 text-text-secondary">Loot TT</td>
								<td class="py-1.5 text-right tabular-nums text-text-secondary">{formatPed(rb.lootTt)}</td>
							</tr>
							{#each Object.entries(rb.ledger) as [tag, amount]}
								{#if amount > 0 && !PROGRESSION_GAIN_TAGS.has(tag)}
									<tr class="border-b border-border/30">
										<td class="py-1.5 pl-5 text-text-secondary">{labelFor(tag)}</td>
										<td class="py-1.5 text-right tabular-nums text-text-secondary">{formatPed(amount)}</td>
									</tr>
								{/if}
							{/each}

							<!-- Spacer -->
							<tr><td class="py-1.5" colspan="2"></td></tr>

							<!-- Costs section -->
							<tr class="border-b border-border">
								<td class="py-2 text-text font-medium">Costs</td>
								<td class="py-2 text-right tabular-nums text-text font-medium">{formatPed(totalCosts)}</td>
							</tr>
							<tr class="border-b border-border/30">
								<td class="py-1.5 pl-5 text-text-secondary">Cycled</td>
								<td class="py-1.5 text-right tabular-nums text-text-secondary">{formatPed(lb.trackingCost)}</td>
							</tr>
							{#if cb.weapon > 0}
								<tr class="border-b border-border/20">
									<td class="py-1 pl-10 text-text-tertiary text-xs">Weapon</td>
									<td class="py-1 text-right tabular-nums text-text-tertiary text-xs">{formatPed(cb.weapon)}</td>
								</tr>
							{/if}
							{#if cb.healing > 0}
								<tr class="border-b border-border/20">
									<td class="py-1 pl-10 text-text-tertiary text-xs">Healing</td>
									<td class="py-1 text-right tabular-nums text-text-tertiary text-xs">{formatPed(cb.healing)}</td>
								</tr>
							{/if}
							{#if cb.enhancer > 0}
								<tr class="border-b border-border/20">
									<td class="py-1 pl-10 text-text-tertiary text-xs">Enhancers</td>
									<td class="py-1 text-right tabular-nums text-text-tertiary text-xs">{formatPed(cb.enhancer)}</td>
								</tr>
							{/if}
							{#if cb.armour > 0}
								<tr class="border-b border-border/20">
									<td class="py-1 pl-10 text-text-tertiary text-xs">Armour</td>
									<td class="py-1 text-right tabular-nums text-text-tertiary text-xs">{formatPed(cb.armour)}</td>
								</tr>
							{/if}
							{#if cb.dangling > 0}
								<tr class="border-b border-border/20">
									<td class="py-1 pl-10 text-text-tertiary text-xs">Dangling</td>
									<td class="py-1 text-right tabular-nums text-text-tertiary text-xs">{formatPed(cb.dangling)}</td>
								</tr>
							{/if}
							{#each Object.entries(lb.ledger) as [tag, amount]}
								{#if amount > 0}
									<tr class="border-b border-border/30">
										<td class="py-1.5 pl-5 text-text-secondary">{labelFor(tag)}</td>
										<td class="py-1.5 text-right tabular-nums text-text-secondary">{formatPed(amount)}</td>
									</tr>
								{/if}
							{/each}

							<!-- Net -->
							<tr><td class="py-1" colspan="2"></td></tr>
							<tr class="border-t border-border">
								<td class="py-2 text-text font-semibold">Net</td>
								<td class="py-2 text-right tabular-nums font-semibold {totalReturns - totalCosts >= 0 ? 'text-positive' : 'text-negative'}">
									{totalReturns - totalCosts >= 0 ? '+' : ''}{formatPed(totalReturns - totalCosts)}
								</td>
							</tr>
						</tbody>
					</table>
				</Card>
				{/if}
			</div>
		{/if}

		<Divider />

		<!-- Monthly breakdown -->
		<div>
			<h3 class="eyebrow mb-3">
				Monthly Breakdown
			</h3>
			{#if monthlyRows.length === 0}
				<p class="text-sm text-text-tertiary">No monthly data yet.</p>
			{:else}
				<table class="w-full text-sm">
					<thead>
						<tr class="border-b border-border">
							<th class="py-2 px-3 text-xs font-medium text-text-secondary text-left">Month</th>
							<th class="py-2 px-3 text-xs font-medium text-text-secondary text-right">Costs</th>
							<th class="py-2 px-3 text-xs font-medium text-text-secondary text-right">Returns</th>
							<th class="py-2 px-3 text-xs font-medium text-text-secondary text-right">Loot Rate</th>
							<th class="py-2 px-3 text-xs font-medium text-text-secondary text-right">Global Rate</th>
							<th class="py-2 px-3 text-xs font-medium text-text-secondary text-right">PES</th>
							<th class="py-2 px-3 text-xs font-medium text-text-secondary text-right">Net</th>
						</tr>
					</thead>
					<tbody>
						{#each monthlyRows as month}
							<tr
								class="border-b border-border/50 hover:bg-surface-hover/50 transition-colors duration-[var(--duration-fast)]"
							>
								<td class="py-2.5 px-3 text-text font-medium">{month.month}</td>
								<td class="py-2.5 px-3 text-right tabular-nums text-text-secondary">
									{formatPed(month.cost)}
								</td>
								<td class="py-2.5 px-3 text-right tabular-nums text-text-secondary">
									{formatPed(month.returns)}
								</td>
								<td class="py-2.5 px-3 text-right tabular-nums text-text">
									{month.lootRate == null ? '—' : formatPercent(month.lootRate)}
								</td>
								<td class="py-2.5 px-3 text-right tabular-nums text-text">
									{month.globalRate == null ? '—' : formatPercent(month.globalRate)}
								</td>
								<td class="py-2.5 px-3 text-right tabular-nums text-text-secondary">
									{formatPed(month.pes)}
								</td>
								<td
									class="py-2.5 px-3 text-right tabular-nums font-medium {month.net >= 0
										? 'text-positive'
										: 'text-negative'}"
								>
									{month.net >= 0 ? '+' : ''}{formatPed(month.net)}
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
			{/if}
		</div>
	</div>
{:else}
	<Card class="p-6">
		<p class="text-sm text-text-tertiary text-center">
			No tracking data yet. Complete a session to see your sustainability overview.
		</p>
	</Card>
{/if}
