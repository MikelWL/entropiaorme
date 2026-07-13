<script lang="ts">
	import type { CharacterModel } from './characterModel.svelte';
	import CodexProfessionPicker from './CodexProfessionPicker.svelte';
	import { targetLabel } from './codexRankingTarget';
	import RecommenderChart from './RecommenderChart.svelte';

	let { model }: { model: CharacterModel } = $props();
	const recommender = $derived(model.recommender);

	const unitLabel = $derived(recommender.target.kind === 'hp' ? 'HP' : 'levels');
	const hasTarget = $derived(recommender.target.kind !== 'none');

	function formatPes(value: number): string {
		return value >= 100 ? value.toFixed(0) : value.toFixed(1);
	}
</script>

<div class="space-y-4">
	<!-- Target row -->
	<div class="flex items-center justify-between gap-3">
		<div class="flex items-center gap-3 min-w-0">
			<span class="text-sm text-text-secondary whitespace-nowrap">Level up</span>
			<CodexProfessionPicker
				professions={model.professions.map((prof) => prof.name)}
				target={recommender.target}
				align="left"
				onchange={(next) => void recommender.load(next)}
			/>
		</div>
		{#if hasTarget && recommender.result}
			<span class="text-xs text-text-tertiary whitespace-nowrap">
				ranked by skilling PES to +1 {unitLabel === 'HP' ? 'HP' : 'level'}
			</span>
		{/if}
	</div>

	{#if !hasTarget}
		<p class="text-sm text-text-tertiary py-8 text-center">
			Pick a profession (or HP) to see which activities level it fastest.
		</p>
	{:else if recommender.loading}
		<p class="text-sm text-text-tertiary py-8 text-center">Projecting activities...</p>
	{:else if recommender.result && recommender.candidates.length === 0}
		<p class="text-sm text-text-tertiary py-8 text-center">
			No activity trains the skills behind {targetLabel(recommender.target)} from your current
			calibration. Scan your skills first if you have not yet.
		</p>
	{:else if recommender.result}
		<div class="grid grid-cols-1 lg:grid-cols-[minmax(15rem,1fr)_2fr] gap-4">
			<!-- Ranked candidates -->
			<div class="max-h-96 overflow-y-auto rounded-md border border-border divide-y divide-border">
				{#each recommender.candidates as candidate (candidate.activity)}
					{@const active = recommender.selected?.activity === candidate.activity}
					<button
						class="w-full px-3 py-2 text-left text-sm flex items-baseline justify-between gap-2 cursor-pointer
							transition-colors {active ? 'bg-accent/10 text-text' : 'text-text-secondary hover:bg-surface/70'}"
						aria-pressed={active}
						onclick={() => recommender.select(candidate.activity)}
					>
						<span class="truncate">{candidate.activity}</span>
						<span class="tabular-nums text-xs whitespace-nowrap {active ? 'text-accent' : 'text-text-tertiary'}">
							{#if candidate.pesToPlusOne !== null}
								+1 in {formatPes(candidate.pesToPlusOne)} PES
							{:else}
								+{candidate.gainAtCap.toFixed(2)} at cap
							{/if}
						</span>
					</button>
				{/each}
			</div>

			<!-- Projection chart + decomposition -->
			<div class="space-y-3 min-w-0">
				{#if recommender.selected}
					<RecommenderChart
						selected={recommender.selected}
						direct={recommender.result.direct}
						pesCap={recommender.result.pesCap}
						sampleStep={recommender.result.sampleStep}
						{unitLabel}
					/>
					{#if recommender.selected.contributors.length > 0}
						<p class="text-xs text-text-tertiary">
							<span class="text-text-secondary">Driven by</span>
							{#each recommender.selected.contributors.slice(0, 4) as contributor, i}
								{i > 0 ? ', ' : ' '}{contributor.name}
								<span class="tabular-nums">(+{contributor.targetGain.toFixed(2)})</span>
							{/each}
							{#if recommender.selected.contributors.length > 4}
								and {recommender.selected.contributors.length - 4} more
							{/if}
						</p>
					{/if}
				{/if}
			</div>
		</div>

		<p class="text-xs text-text-tertiary">
			Projections follow the profession weight vectors and your current skill levels, per unit of
			skilling PES; how much PES an hour of each activity yields is not modelled. The faded line
			is the target's own activity, shown for reference: some professions have no direct grind
			path.
		</p>
	{/if}
</div>
