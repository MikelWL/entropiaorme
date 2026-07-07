/**
 * Quest and playlist economics: the pure derivations behind the quests
 * analytics view. No runes; every function is a plain input-to-output
 * mapping so the accounting invariants (liquid TT and non-liquid PES
 * never blend) stay pinned by the colocated tests.
 */

import type { AnalyticsOverview } from '$lib/api/commands.gen';
import type { PlaylistAnalyticsRow, QuestAnalyticsRow } from '$lib/types';

export type RewardMode = 'tt' | 'markup';

export interface GlobalRates {
	/**
	 * Liquid PED returns per cycled PED: drives Net/Rate forecasts and the
	 * "PES burn-saved" translation. Skill TT, codex PES, and quest PES are
	 * progression and stay out of this rate.
	 */
	liquidReturnRate: number;
	/**
	 * Combined PES throughput per cycled PED (skill TT + codex PES + quest PES).
	 * Used to translate a PES reward into the equivalent cycle it would replace.
	 */
	skillProgressionRate: number;
}

/** Derive the global baseline rates from the all-time analytics overview. */
export function globalRates(
	overview: Pick<AnalyticsOverview, 'returnsBreakdown' | 'lossesBreakdown'>,
): GlobalRates {
	// Liquid PED returns: TT loot plus liquid ledger gains (convert).
	// Quest reward markup is per-quest in `total_expected_reward_ped`
	// so it's intentionally not folded into the global rate here.
	const convertGains = overview.returnsBreakdown.ledger.convert ?? 0;
	const liquidReturns = overview.returnsBreakdown.lootTt + convertGains;
	// PES throughput across all progression channels.
	const skillProgressionReturns =
		overview.returnsBreakdown.pes +
		(overview.returnsBreakdown.codexPes ?? 0) +
		(overview.returnsBreakdown.questPes ?? 0);
	const rawCycled = overview.lossesBreakdown.trackingCost;
	return {
		liquidReturnRate: rawCycled > 0 ? liquidReturns / rawCycled : 0,
		skillProgressionRate: rawCycled > 0 ? skillProgressionReturns / rawCycled : 0,
	};
}

export interface QuestAnalyticsComputed {
	questId: string;
	questName: string;
	planet: string;
	category: string | null;
	rewardPed: number;
	rewardIsSkill: boolean;
	expectedRewardMarkupPercent: number | null;
	linkedSessions: number;
	totalCycled: number;
	avgRawReturns: number;
	avgCycled: number;
	// Liquid PED reward shown in the Reward column. Toggle-aware: TT mode
	// = face value, Markup mode = with expected markup applied. 0 for
	// skill quests since they have no liquid contribution.
	displayLiquidReward: number;
	// PES face value of the reward, invariant to toggle. 0 for liquid quests.
	avgRewardPes: number;
	rewardMarkupPercent: number | null;
	// Liquid Net for the run: liquid cycle returns + liquid reward − cycled.
	avgNet: number;
	// Cycle PES baseline + explicit PES reward. Always at face value.
	avgPesNet: number;
	returnRate: number;
}

export function computeQuestAnalytics(
	rows: QuestAnalyticsRow[],
	rates: GlobalRates,
	rewardMode: RewardMode,
): QuestAnalyticsComputed[] {
	return rows.map((row) => {
		const totalCycled =
			row.totalWeaponCost + row.totalHealCost + row.totalEnhancerCost + row.totalArmourCost;
		const totalReward = row.rewardPed * row.linkedSessions;
		const sessions = row.linkedSessions || 1;
		const avgCycled = totalCycled / sessions;
		// Liquid reward: face value or with markup, depending on toggle.
		// Skill quests contribute 0 to the liquid side regardless.
		const avgRewardLiquidFace = row.rewardIsSkill ? 0 : totalReward / sessions;
		const avgRewardLiquidMarkup = row.rewardIsSkill ? 0 : row.totalExpectedRewardPed / sessions;
		const displayLiquidReward =
			rewardMode === 'markup' ? avgRewardLiquidMarkup : avgRewardLiquidFace;
		// PES reward stays at face value across both modes.
		const avgRewardPes = row.rewardIsSkill ? row.rewardPed : 0;
		// Liquid cycle projection (PES sources excluded: denomination-pure).
		const avgRawReturns = avgCycled * rates.liquidReturnRate;
		const avgNet = avgRawReturns + displayLiquidReward - avgCycled;
		const returnRate = avgCycled > 0 ? (avgRawReturns + displayLiquidReward) / avgCycled : 0;
		// PES cycle baseline + explicit PES reward (face value).
		const avgPesNet = avgCycled * rates.skillProgressionRate + avgRewardPes;
		return {
			questId: row.questId,
			questName: row.questName,
			planet: row.planet,
			category: row.category,
			rewardPed: row.rewardPed,
			rewardIsSkill: row.rewardIsSkill,
			expectedRewardMarkupPercent: row.expectedRewardMarkupPercent,
			linkedSessions: row.linkedSessions,
			totalCycled,
			displayLiquidReward,
			avgRewardPes,
			rewardMarkupPercent: row.expectedRewardMarkupPercent,
			avgRawReturns,
			avgCycled,
			avgNet,
			avgPesNet,
			returnRate,
		};
	});
}

export interface PlaylistAnalyticsComputed {
	playlistName: string;
	questCount: number;
	longHorizonQuestCount: number;
	// Toggle-aware liquid display (face value or markup-applied).
	displayImmediateReward: number;
	displayBonusReward: number;
	// PES face-value sub-line totals, invariant to toggle.
	avgImmediateSkillReward: number;
	avgBonusSkillReward: number;
	rewardMarkupPercent: number | null;
	avgCycled: number;
	avgRawReturns: number;
	// Liquid Net + cycle-PES Net (face value).
	avgNet: number;
	avgPesNet: number;
	returnRate: number;
}

export function computePlaylistAnalytics(
	rows: PlaylistAnalyticsRow[],
	rates: GlobalRates,
	rewardMode: RewardMode,
): PlaylistAnalyticsComputed[] {
	return rows.map((row) => {
		const totalCycled =
			row.totalWeaponCost + row.totalHealCost + row.totalEnhancerCost + row.totalArmourCost;
		const sessions = row.matchedSessions || 1;
		const avgImmediateReward = row.totalImmediateRewardPed / sessions;
		const avgBonusReward = row.totalBonusRewardPed / sessions;
		const avgImmediateSkillReward = row.totalImmediatePesReward / sessions;
		const avgBonusSkillReward = row.totalBonusPesReward / sessions;
		// Liquid portions (face value).
		const avgImmediateLiquidFace = avgImmediateReward - avgImmediateSkillReward;
		const avgBonusLiquidFace = avgBonusReward - avgBonusSkillReward;
		// Liquid portions with expected markup applied. Backend already
		// emits face value for skill quests in the expected totals, so
		// subtracting the PES sum yields the liquid-with-markup amount.
		const avgImmediateLiquidMarkup =
			(row.totalExpectedImmediateRewardPed - row.totalImmediatePesReward) / sessions;
		const avgBonusLiquidMarkup =
			(row.totalExpectedBonusRewardPed - row.totalBonusPesReward) / sessions;
		const displayImmediateReward =
			rewardMode === 'markup' ? avgImmediateLiquidMarkup : avgImmediateLiquidFace;
		const displayBonusReward = rewardMode === 'markup' ? avgBonusLiquidMarkup : avgBonusLiquidFace;
		const liquidFaceTotal = avgImmediateLiquidFace + avgBonusLiquidFace;
		const liquidMarkupTotal = avgImmediateLiquidMarkup + avgBonusLiquidMarkup;
		const rewardMarkupPercentValue =
			liquidFaceTotal > 0 ? (liquidMarkupTotal / liquidFaceTotal) * 100 : null;
		const avgCycled = totalCycled / sessions;
		const avgRawReturns = avgCycled * rates.liquidReturnRate;
		const avgNet = avgRawReturns + displayImmediateReward + displayBonusReward - avgCycled;
		const returnRate =
			avgCycled > 0 ? (avgRawReturns + displayImmediateReward + displayBonusReward) / avgCycled : 0;
		const avgPesNet =
			avgCycled * rates.skillProgressionRate + avgImmediateSkillReward + avgBonusSkillReward;
		return {
			playlistName: row.playlistName,
			questCount: row.questCount,
			longHorizonQuestCount: row.longHorizonQuestCount,
			displayImmediateReward,
			displayBonusReward,
			avgImmediateSkillReward,
			avgBonusSkillReward,
			rewardMarkupPercent: rewardMarkupPercentValue,
			avgCycled,
			avgRawReturns,
			avgNet,
			avgPesNet,
			returnRate,
		};
	});
}
