<script lang="ts">
	import type { CodexSkillOption } from '$lib/types/analytics';
	import { formatPedHalfEven } from '$lib/utils/format';

	let {
		options,
		rankedBy,
		onClaim,
		mastery = false,
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
		justClaimedSkill?: string | null;
	}>();
</script>

<div class="space-y-0.5">
	{#each options as opt}
		{@const rank = opt.recommendRank}
		<div class="flex items-center justify-between py-1.5 px-2 rounded hover:bg-surface-hover/50 transition-colors group">
			<div class="flex items-center gap-2 min-w-0">
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
			<div class="flex items-center gap-3 shrink-0 ml-2">
				{#if mastery}
					<span class="text-xs tabular-nums text-text font-medium">
						{formatPedHalfEven(opt.rewardPed)} PES
					</span>
				{/if}
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
				{:else if opt.professionWeight > 0}
					<div class="text-right text-xs tabular-nums">
						<span class="text-text-secondary">+{opt.levelsGained.toFixed(1)} lvl</span>
						<span class="text-text-tertiary mx-0.5">&times;</span>
						<span class="text-text-secondary">w{opt.professionWeight}</span>
						{#if opt.profContribution > 0}
							<span class="text-accent font-medium ml-1">= +{(opt.profContribution * 100).toFixed(3)}%</span>
						{/if}
					</div>
				{:else if opt.currentLevel != null}
					<span class="text-xs text-text-tertiary tabular-nums">Lv {opt.currentLevel.toFixed(0)}, +{opt.levelsGained.toFixed(1)}</span>
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
