<script lang="ts">
	import type { CodexSkillOption } from '$lib/types/analytics';
	import { formatPedHalfEven } from '$lib/utils/format';

	let {
		options,
		rankedBy,
		onClaim,
		mastery = false,
		family = false,
		justClaimedSkill = null,
	} = $props<{
		options: CodexSkillOption[];
		rankedBy: 'hp' | 'profession' | null;
		onClaim: (skillName: string) => void;
		/** Mastery rewards vary per skill (no single header value applies),
		 *  so mastery rows carry their own value and acknowledge a claim
		 *  in place: the mastery panel stays on screen after a claim,
		 *  unlike a rank claim which advances the whole card. */
		mastery?: boolean;
		/** A family target always reads as the per-profession split, even
		 *  when only one member profession is affected by a skill. */
		family?: boolean;
		justClaimedSkill?: string | null;
	}>();
</script>

<div class="space-y-0.5">
	{#each options as opt}
		{@const rank = opt.recommendRank}
		{@const familyStrip = rankedBy !== 'hp' && family && opt.professionContributions.length > 0}
		<div class="flex items-center justify-between py-1.5 px-2 rounded hover:bg-surface-hover/50 transition-colors group">
			<div class="flex items-center gap-2 min-w-0 shrink-0">
				{#if rank != null}
					<span class="text-xs font-medium tabular-nums w-5 text-center shrink-0
						{rank === 1 ? 'text-success' : rank <= 3 ? 'text-accent' : 'text-text-tertiary'}">
						#{rank}
					</span>
				{:else}
					<span class="w-5 shrink-0"></span>
				{/if}
				<span class="text-sm text-text truncate">{opt.skillName}</span>
			</div>
			<div class="flex items-center gap-3 ml-2 {familyStrip ? 'min-w-0 flex-1 justify-end' : 'shrink-0'}">
				{#if rankedBy === 'hp'}
					<div class="text-right text-xs tabular-nums">
						{#if opt.hpIncrease != null}
							<span class="text-text-secondary">+{opt.levelsGained.toFixed(1)} lvl</span>
							<span class="text-text-tertiary mx-0.5">/</span>
							<span class="text-text-secondary">{opt.hpIncrease.toFixed(0)} lvl/HP</span>
							<span class="text-accent font-medium ml-1">= +{opt.hpGain.toFixed(3)} HP</span>
						{:else}
							<span class="text-text-tertiary">No HP gain</span>
						{/if}
					</div>
				{:else if familyStrip}
					<!-- A family target reads as the per-profession split; each
					     profession stays one unbreakable block so wrapping
					     never splits a name from its figure. -->
					<div class="flex flex-wrap justify-end gap-x-3 gap-y-0.5 text-right text-xs tabular-nums">
						{#each opt.professionContributions as entry}
							<span class="whitespace-nowrap">
								<span class="text-text-secondary">{entry.profession}:</span>
								<span class="text-accent font-medium">+{(entry.profContribution * 100).toFixed(3)}%</span>
							</span>
						{/each}
					</div>
				{:else if rankedBy === 'profession' && opt.professionWeight > 0}
					<div class="text-right text-xs tabular-nums">
						<span class="text-text-secondary">+{opt.levelsGained.toFixed(1)} lvl</span>
						<span class="text-text-tertiary mx-0.5">&times;</span>
						<span class="text-text-secondary">w{opt.professionWeight}</span>
						{#if opt.profContribution > 0}
							<span class="text-accent font-medium ml-1">= +{(opt.profContribution * 100).toFixed(3)}%</span>
						{/if}
					</div>
				{:else if rankedBy === 'profession'}
					<!-- The profession-mode sibling of "No HP gain". -->
					<span class="text-xs text-text-tertiary">No contribution</span>
				{:else if opt.currentLevel != null}
					<span class="text-xs text-text-tertiary tabular-nums">Lv {opt.currentLevel.toFixed(0)}, +{opt.levelsGained.toFixed(1)}</span>
				{/if}
				{#if mastery}
					<!-- After the contribution info, so the values right-align
					     as one column however many professions a row lists. -->
					<span class="text-xs tabular-nums text-text font-medium shrink-0 w-20 text-right">
						{formatPedHalfEven(opt.rewardPed)} PES
					</span>
				{/if}
				{#if mastery && justClaimedSkill === opt.skillName}
					<span class="px-2 py-1 text-xs font-medium text-positive">Claimed &check;</span>
				{:else}
					<button
						class="px-2 py-1 text-xs font-medium text-accent bg-accent/10 hover:bg-accent/20 rounded transition-colors cursor-pointer opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
						onclick={() => onClaim(opt.skillName)}
					>Claim</button>
				{/if}
			</div>
		</div>
	{/each}
</div>

<p class="text-xs text-text-tertiary">
	{#if rankedBy === 'hp' && options.some((o: CodexSkillOption) => o.recommendRank === 1)}
		Ranked by expected HP gain from this codex reward at your current level. HP gain uses the skill's HP increase stat: every N skill levels adds 1 HP.
	{:else if rankedBy === 'profession' && options.some((o: CodexSkillOption) => o.recommendRank === 1)}
		Ranked by profession contribution: +levels &times; weight. Accounts for diminishing returns at higher skill levels.
	{/if}
</p>
