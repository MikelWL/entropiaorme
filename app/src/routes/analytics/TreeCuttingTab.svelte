<script lang="ts">
	import Card from '$lib/components/Card.svelte';
	import ErrorNotice from '$lib/components/ErrorNotice.svelte';
	import InfoTip from '$lib/components/InfoTip.svelte';
	import SegmentedControl from '$lib/components/SegmentedControl.svelte';
	import StatDisplay from '$lib/components/StatDisplay.svelte';
	import {
		type ConfidenceMode,
		createTreeCuttingModel,
		type TreeCuttingItem,
	} from '$lib/features/analytics/treeCuttingModel.svelte';
	import { formatPed, formatPercent } from '$lib/utils/format';

	const model = createTreeCuttingModel();

	$effect(() => {
		void model.loadData();
	});

	const NO_DATA = '—';

	// Net figures follow the app's "Net" precedent (Overview): sign-aware
	// text, positive/negative tone. The realised Net is toned; the MU Net
	// stays neutral (it is estimated, so it is not painted as banked gain).
	const signedPed = (v: number) => `${v >= 0 ? '+' : ''}${formatPed(v)}`;
	const netTone = (v: number) => (v >= 0 ? 'text-positive' : 'text-negative');

	// User-facing framing is trading-volume, not the internal tier names:
	// each option widens which items' own markup is trusted by how readily
	// the market can absorb the player's looted position.
	const MODE_OPTIONS: { id: ConfidenceMode; label: string }[] = [
		{ id: 'liquid', label: 'High Vol. Only' },
		{ id: 'liquidMiddling', label: 'High & Mid Vol.' },
		{ id: 'all', label: 'High, Mid & Low Vol.' },
	];

	// Whether a section carries market context at all (null MU = the
	// market feed was unavailable, so markup cells stay blank).
	function hasMarket(mu: number | null): boolean {
		return mu !== null;
	}

	// The per-item confidence explanation, split into a lead (the headline
	// signal, emphasised) and a detail line. A fallback horizon leads with
	// the absence of weekly sales rather than a weekly figure normalised
	// down from the month/year, which would understate how thin it is.
	function confidenceTip(item: TreeCuttingItem): { lead: string; detail?: string } {
		const floorNote = item.floored ? ' Valued at the nanocube rate instead.' : '';
		const pos = `Your position: ${formatPed(item.positionTt)} PED.`;

		if (item.ownMarkupPct == null) {
			return { lead: 'No market data for this item.', detail: floorNote.trim() || undefined };
		}

		if (item.markupHorizon && item.markupHorizon !== 'week') {
			const weekly = item.weeklySalesPed;
			const lead =
				weekly == null || weekly <= 0
					? 'No sales in the last week.'
					: `Only ${formatPed(weekly)} PED sold last week.`;
			const range =
				item.salesPed != null
					? `Priced from the last ${item.markupHorizon} (${formatPed(item.salesPed)} PED traded). `
					: '';
			return { lead, detail: `${range}${pos}${floorNote}` };
		}

		const vol = `~${formatPed(item.salesPed ?? 0)} PED traded last week`;
		if (item.tier === 'liquid') {
			return { lead: `High volume: ${vol}.`, detail: `${pos} A small share, so this markup is realistic.` };
		}
		if (item.tier === 'middling') {
			return {
				lead: `Medium volume: ${vol}.`,
				detail: `${pos} A sizeable share, so selling it all at this markup may be difficult.${floorNote}`,
			};
		}
		return {
			lead: `Low volume: ${vol}.`,
			detail: `${pos} May not sell at this markup.${floorNote}`,
		};
	}
</script>

{#snippet statGrid(g: {
	title: string;
	subtitle: string | null;
	cycled: number;
	returns: number;
	lootRate: number;
	muProjected: number | null;
	muRate: number | null;
})}
	<!-- Per-tool stat area as a 2x3 grid: the title anchors the top-left
		cell as the box's heading, MU figures fill out row 1, realised stats
		sit in row 2. Net = returns - cycled (the app's "Net" precedent). -->
	<div class="grid grid-cols-2 gap-x-4 gap-y-4 sm:grid-cols-3">
		<div
			class="col-span-2 sm:col-span-1 flex flex-col justify-center gap-0.5
				rounded-lg border-l-2 border-accent bg-accent/[0.05] px-3.5 py-2"
		>
			<span class="text-lg font-semibold tracking-tight leading-tight text-text">
				{g.title}
			</span>
			{#if g.subtitle}
				<span class="text-xs text-text-secondary">{g.subtitle}</span>
			{/if}
		</div>

		<StatDisplay
			label="MU Net"
			value={g.muProjected !== null ? signedPed(g.muProjected - g.cycled) : NO_DATA}
			unit={g.muProjected !== null ? 'PED' : ''}
		/>
		<StatDisplay
			label="MU Rate"
			value={g.muRate !== null ? formatPercent(g.muRate) : NO_DATA}
		/>

		<StatDisplay label="Cycled" value={formatPed(g.cycled)} unit="PED" />
		<StatDisplay
			label="Net"
			value={signedPed(g.returns - g.cycled)}
			valueClass={netTone(g.returns - g.cycled)}
			unit="PED"
		/>
		<StatDisplay label="Rate" value={formatPercent(g.lootRate)} />
	</div>
{/snippet}

{#snippet actionButton(letter: string, label: string, expandedWidth: string)}
	<!-- Placeholder action: collapsed to its initial, expanding on hover to
		the full label (letter and label cross-fade as the pill grows). Wired
		to the sell / recycle flow in a later pass. -->
	<button
		type="button"
		aria-label={label}
		class="group/act relative inline-flex h-6 w-6 shrink-0 items-center justify-center overflow-hidden
			rounded-md border border-border/60 bg-surface-hover/40 text-xs font-semibold text-text-secondary
			transition-[width,background-color,color,border-color] duration-[var(--duration-base)] ease-[var(--ease-out)]
			{expandedWidth} hover:text-text hover:border-border hover:bg-surface-hover/70"
	>
		<span
			class="absolute inset-0 flex items-center justify-center
				transition-opacity duration-[var(--duration-fast)] group-hover/act:opacity-0"
		>
			{letter}
		</span>
		<span
			class="absolute inset-0 flex items-center justify-center whitespace-nowrap px-2
				opacity-0 transition-opacity duration-[var(--duration-fast)] group-hover/act:opacity-100"
		>
			{label}
		</span>
	</button>
{/snippet}

{#snippet confidenceBody(item: TreeCuttingItem)}
	{@const tip = confidenceTip(item)}
	<p class="text-xs leading-relaxed text-text">{tip.lead}</p>
	{#if tip.detail}
		<p class="mt-1 text-xs leading-relaxed text-text-secondary">{tip.detail}</p>
	{/if}
{/snippet}

{#if model.loading}
	<p class="text-sm text-text-secondary">Loading tree cutting data...</p>
{:else if model.error && !model.sections.length}
	<ErrorNotice message={model.error} />
{:else if model.sections.length > 0}
	<div class="space-y-5" data-guide-anchor="analytics-treecutting-area">
		<ErrorNotice message={model.error} />

		<!-- Markup-confidence toggle: right-hung, explanation behind an info tip -->
		<div class="flex items-center justify-end gap-2.5">
			<span class="eyebrow">Markup confidence</span>
			<InfoTip label="How markup confidence works">
				<div class="space-y-2 text-xs leading-relaxed text-text-secondary">
					<p class="text-text">
						Sets which items count toward the MU figures, based on how much of each
						item the market buys.
					</p>
					<ul class="space-y-1.5">
						<li>
							<span class="text-text font-medium">High Vol.</span> sells easily, so you
							should get that MU.
						</li>
						<li>
							<span class="text-text font-medium">Mid Vol.</span> you hold a lot compared
							to what sells, so selling all your loot and getting that MU may be
							difficult.
						</li>
						<li>
							<span class="text-text font-medium">Low Vol.</span> barely sells, so you
							likely can't get that MU.
						</li>
					</ul>
					<p>
						Items you leave out are valued at what you'd get by recycling them into
						nanocubes instead, with their MU struck through.
					</p>
				</div>
			</InfoTip>
			<SegmentedControl
				options={MODE_OPTIONS}
				active={model.confidenceMode}
				onchange={(id) => (model.confidenceMode = id as ConfidenceMode)}
			/>
		</div>

		<!-- Overall: the header box the per-tool cards are anchored beneath. A
			distinct elevated treatment (accent wash + border + shadow) sets it
			apart as the summary the subordinate tool cards explain. Two
			columns: the headline stats (Cycled / Net / MU Net) left, current
			stock right. hover:z-20 keeps its stock info tip above the card
			below. -->
		{#if model.overall}
			<div
				class="relative hover:z-20 rounded-xl border border-accent/30 p-6 shadow-lg
					backdrop-blur-[2px] bg-gradient-to-br from-accent/[0.12] via-surface/70 to-surface/70"
			>
				<div class="grid gap-x-8 gap-y-6 sm:grid-cols-[auto_1fr]">
					<!-- Headline stats, stacked top-down -->
					<div class="flex flex-col gap-4">
						<div class="flex flex-col gap-0.5">
							<span class="text-2xl font-semibold tracking-tight leading-none text-text">
								Overall
							</span>
							<span class="eyebrow text-accent/70">All tools</span>
						</div>
						<StatDisplay label="Cycled" value={formatPed(model.overall.cycled)} unit="PED" />
						<StatDisplay
							label="Net"
							value={signedPed(model.overall.returns - model.overall.cycled)}
							valueClass={netTone(model.overall.returns - model.overall.cycled)}
							unit="PED"
						/>
						<StatDisplay
							label="MU Net"
							value={model.overall.muProjectedReturns !== null
								? signedPed(model.overall.muProjectedReturns - model.overall.cycled)
								: NO_DATA}
							unit={model.overall.muProjectedReturns !== null ? 'PED' : ''}
						/>
					</div>

					<!-- Current stock: the market-position overlay on recorded
						harvest. Held quantity drives markup confidence, never the
						stat lines beside it.

						Held is display-only for now: hand-editing a position is
						deliberately switched off until current stock is driven
						automatically from recorded sales, so the two cannot drift
						apart by hand in the meantime. The full write path behind it
						(model.setHeld -> the stored removed quantity) is kept intact
						and ready to switch back on when that automatic link lands. -->
					{#if model.stock.length > 0}
						<div class="sm:border-l sm:border-border/40 sm:pl-8">
							<div class="flex items-center gap-2 pb-2">
								<span class="eyebrow">Current stock</span>
								<InfoTip align="right" label="What current stock means">
									<div class="space-y-2 text-xs leading-relaxed text-text-secondary">
										<p class="text-text">
											How much of each item you still hold, out of everything you have
											recorded harvesting.
										</p>
										<p>
											This is what markup confidence uses: holding less shifts which
											markups are realistic. It never changes the stats beside it, which
											record what your harvesting actually produced.
										</p>
										<p>
											For now this shows everything you have harvested; it will start
											to track what you have sold once recorded sales feed into it.
										</p>
									</div>
								</InfoTip>
							</div>

							<div class="flex items-center gap-3 px-2.5 pb-1 text-text-tertiary">
								<span class="eyebrow flex-1 min-w-0">Item</span>
								<span class="eyebrow w-24 text-right shrink-0">Stock TT</span>
								<span class="eyebrow w-20 text-right shrink-0">Markup</span>
								<span class="eyebrow w-12 text-center shrink-0">Conf</span>
								<span class="w-[3.375rem] shrink-0"></span>
							</div>

							<ul class="flex flex-col gap-1">
								{#each model.stock as s (s.itemName)}
									<li class="flex items-center gap-3 rounded-md px-2.5 py-2">
										<span class="flex-1 min-w-0 text-sm font-medium truncate tracking-tight text-text">
											{s.itemName}
										</span>

										<!-- Stock TT: the market position, sorted on. -->
										<span class="w-24 text-right shrink-0 text-sm tabular-nums font-medium text-text">
											{formatPed(s.heldTt)}
										</span>

										<!-- Markup (placeholder): weekly markup with month/year
											fallback, wired next. Designed to become a dynamic cell
											whose hover opens a window of markup across day/week/
											month/year plus trading volume and the reading's age. -->
										<span class="w-20 text-right shrink-0 text-sm tabular-nums text-text-tertiary">
											—
										</span>

										<!-- Conf (placeholder): the confidence marker, wired next
											(green tick / amber warning / red low, each with a hover
											explanation). -->
										<span class="w-12 text-center shrink-0 text-sm text-text-tertiary">
											—
										</span>

										<!-- Actions (placeholders): recycle to nanocube, and sell.
											Auto-width so a button expanding on hover is absorbed by
											the name column rather than overflowing. -->
										<div class="shrink-0 flex items-center justify-end gap-1.5">
											{@render actionButton('N', 'Turn into Nanocube', 'hover:w-44')}
											{@render actionButton('S', 'Sell', 'hover:w-16')}
										</div>
									</li>
								{/each}
							</ul>
						</div>
					{/if}
				</div>
			</div>
		{/if}

		{#each model.sections as section (section.toolName)}
			<!-- hover:z-20 lifts the whole card above later sibling cards so a
				row tooltip that overflows the card bottom is not painted behind
				the next one (each card is its own stacking context via
				backdrop-blur, so the tooltip's own z-index can't escape). -->
			<Card class="p-5 hover:z-20">
				{@render statGrid({
					title: section.tree ? `${section.tree} Trees` : section.toolName,
					subtitle: section.tree ? section.toolName : null,
					cycled: section.cycled,
					returns: section.returns,
					lootRate: section.lootRate,
					muProjected: section.muProjectedReturns,
					muRate: section.muRate,
				})}

				<!-- Per-item breakdown -->
				{#if section.items.length > 0}
					<div class="mt-5 border-t border-border/50 pt-4">
						<div class="flex items-center gap-3 px-2.5 pb-1 text-text-tertiary">
							<span class="eyebrow flex-1 min-w-0">Item</span>
							<span class="eyebrow w-20 text-right shrink-0">TT</span>
							<span class="eyebrow w-14 text-right shrink-0">Share</span>
							<span class="eyebrow w-36 text-right shrink-0">Markup</span>
						</div>

						<ul class="flex flex-col gap-1">
							{#each section.items as item (item.name)}
								<li
									class="flex items-center gap-3 rounded-md px-2.5 py-2 border border-transparent
										hover:bg-surface-hover/30 hover:border-border/40
										transition-[background-color,border-color] duration-[var(--duration-base)] ease-[var(--ease-out)]"
								>
									<div class="flex-1 min-w-0 flex items-baseline gap-2">
										<span class="text-sm font-medium truncate tracking-tight text-text">
											{item.name}
										</span>
										<span class="text-xs text-text-tertiary tabular-nums shrink-0">
											×{item.quantity}
										</span>
									</div>

									<span class="text-sm tabular-nums font-medium text-text shrink-0 w-20 text-right">
										{formatPed(item.ttValue)}
									</span>

									<span
										class="text-sm tabular-nums font-semibold text-accent shrink-0 w-14 text-right tracking-tight"
									>
										{item.sharePct.toFixed(1)}%
									</span>

									<!-- Markup: neutral number + a separate confidence glyph
										carrying its own hover explanation; floored markups
										are struck through and shown at the nanocube rate. -->
									<span
										class="text-sm tabular-nums shrink-0 w-36 text-right flex items-center justify-end gap-1.5"
									>
										{#if !hasMarket(section.muProjectedReturns)}
											<span class="text-text-tertiary">{NO_DATA}</span>
										{:else}
											{#if item.tier === 'middling'}
												<InfoTip align="right" label="Medium volume">
													{#snippet trigger()}
														<span class="text-warning">⚠</span>
													{/snippet}
													{@render confidenceBody(item)}
												</InfoTip>
											{:else if item.tier === 'illiquid'}
												<InfoTip align="right" label="Low volume">
													{#snippet trigger()}
														<span class="text-error font-semibold">!</span>
													{/snippet}
													{@render confidenceBody(item)}
												</InfoTip>
											{/if}
											{#if item.floored && item.ownMarkupPct !== null}
												<span class="text-text-tertiary line-through">
													{formatPercent(item.ownMarkupPct / 100)}
												</span>
												<span class="text-text-secondary">
													{formatPercent(item.effectiveMarkupPct / 100)}
												</span>
											{:else}
												<span class="text-text-secondary">
													{formatPercent(item.effectiveMarkupPct / 100)}
												</span>
											{/if}
										{/if}
									</span>
								</li>
							{/each}
						</ul>
					</div>
				{:else}
					<p class="mt-4 text-xs text-text-tertiary px-2.5">
						No loot recorded on this tool yet.
					</p>
				{/if}
			</Card>
		{/each}

		<div class="space-y-1 text-xs text-text-tertiary">
			<p>
				<span class="text-text-secondary">Net / Rate:</span>
				realised loot TT minus cycled PED, and loot-only TT return per cycled PED.
			</p>
			<p>
				<span class="text-text-secondary">MU Net / MU figures:</span>
				estimated from market data, never realised P&L. Markup resolves from the weekly
				horizon (falling back to monthly, then yearly). A
				<span class="text-warning">⚠</span> flags a markup the market may only partly absorb;
				a <span class="text-error font-semibold">!</span> flags one that likely cannot be sold
				at that rate, shown struck through with the nanocube recycling floor.
			</p>
		</div>
	</div>
{:else}
	<Card class="p-6">
		<p class="text-sm text-text-tertiary text-center" data-guide-anchor="analytics-treecutting-area">
			No tree cutting data yet. Harvest trees during a tracked session to see per-tool sections.
		</p>
	</Card>
{/if}
