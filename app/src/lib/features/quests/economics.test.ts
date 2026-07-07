import { describe, expect, it } from 'vitest';
import type { PlaylistAnalyticsRow, QuestAnalyticsRow } from '$lib/types';
import {
	computePlaylistAnalytics,
	computeQuestAnalytics,
	type GlobalRates,
	globalRates,
} from './economics';

// 0.9 liquid PED and 0.05 PES back per cycled PED: round numbers so every
// expected value below is exact by hand.
const RATES: GlobalRates = { liquidReturnRate: 0.9, skillProgressionRate: 0.05 };

const emptyCycled = { weapon: 0, healing: 0, enhancer: 0, armour: 0, dangling: 0 };

function questRow(overrides: Partial<QuestAnalyticsRow> = {}): QuestAnalyticsRow {
	return {
		questId: 'q1',
		questName: 'Daily Kill',
		planet: 'Calypso',
		category: null,
		rewardPed: 2,
		rewardIsSkill: false,
		expectedRewardMarkupPercent: 130,
		totalExpectedRewardPed: 5.2,
		linkedSessions: 2,
		totalDurationSec: 3600,
		totalWeaponCost: 60,
		totalHealCost: 20,
		totalEnhancerCost: 15,
		totalArmourCost: 5,
		totalLootTt: 90,
		totalPes: 4,
		...overrides,
	};
}

function playlistRow(overrides: Partial<PlaylistAnalyticsRow> = {}): PlaylistAnalyticsRow {
	return {
		playlistId: 'pl1',
		playlistName: 'Daily run',
		questCount: 3,
		longHorizonQuestCount: 1,
		matchedSessions: 2,
		totalRewardPed: 16,
		totalImmediateRewardPed: 10,
		totalBonusRewardPed: 6,
		totalPesReward: 6,
		totalImmediatePesReward: 4,
		totalBonusPesReward: 2,
		totalExpectedRewardPed: 19,
		totalExpectedImmediateRewardPed: 11.8,
		totalExpectedBonusRewardPed: 7.2,
		totalDurationSec: 3600,
		totalWeaponCost: 60,
		totalHealCost: 20,
		totalEnhancerCost: 15,
		totalArmourCost: 5,
		totalLootTt: 90,
		totalPes: 4,
		...overrides,
	};
}

describe('globalRates', () => {
	it('derives the liquid rate from loot TT plus ledger convert gains only', () => {
		const rates = globalRates({
			returnsBreakdown: { lootTt: 80, pes: 3, codexPes: 1, questPes: 2, ledger: { convert: 10 } },
			lossesBreakdown: { trackingCost: 100, cycledBreakdown: emptyCycled, ledger: {} },
		});
		// (80 + 10) / 100: PES channels are progression and must not leak in.
		expect(rates.liquidReturnRate).toBeCloseTo(0.9, 12);
		// (3 + 1 + 2) / 100: all three progression channels, none of the liquid.
		expect(rates.skillProgressionRate).toBeCloseTo(0.06, 12);
	});

	it('treats a missing ledger convert bucket as zero', () => {
		const rates = globalRates({
			returnsBreakdown: { lootTt: 50, pes: 0, codexPes: 0, questPes: 0, ledger: {} },
			lossesBreakdown: { trackingCost: 100, cycledBreakdown: emptyCycled, ledger: {} },
		});
		expect(rates.liquidReturnRate).toBeCloseTo(0.5, 12);
	});

	it('returns zero rates when nothing has been cycled', () => {
		const rates = globalRates({
			returnsBreakdown: { lootTt: 80, pes: 3, codexPes: 1, questPes: 2, ledger: { convert: 10 } },
			lossesBreakdown: { trackingCost: 0, cycledBreakdown: emptyCycled, ledger: {} },
		});
		expect(rates.liquidReturnRate).toBe(0);
		expect(rates.skillProgressionRate).toBe(0);
	});
});

describe('computeQuestAnalytics: liquid quests', () => {
	// Fixture: 2 PED face reward, 130% expected markup, 2 sessions, 100 PED
	// cycled in total (so 50 per session).
	it('shows the face-value reward in TT mode', () => {
		const [row] = computeQuestAnalytics([questRow()], RATES, 'tt');
		expect(row.avgCycled).toBeCloseTo(50, 12);
		expect(row.displayLiquidReward).toBeCloseTo(2, 12);
		// 50 * 0.9 cycle returns + 2 reward - 50 cycled.
		expect(row.avgRawReturns).toBeCloseTo(45, 12);
		expect(row.avgNet).toBeCloseTo(-3, 12);
		expect(row.returnRate).toBeCloseTo(0.94, 12);
	});

	it('shows the markup-applied reward in markup mode and passes the markup through', () => {
		const [row] = computeQuestAnalytics([questRow()], RATES, 'markup');
		// 5.2 expected total over 2 sessions.
		expect(row.displayLiquidReward).toBeCloseTo(2.6, 12);
		expect(row.avgNet).toBeCloseTo(-2.4, 12);
		expect(row.returnRate).toBeCloseTo(0.952, 12);
		expect(row.rewardMarkupPercent).toBe(130);
	});

	it('keeps the PES column at zero for a liquid quest in both modes', () => {
		for (const mode of ['tt', 'markup'] as const) {
			const [row] = computeQuestAnalytics([questRow()], RATES, mode);
			expect(row.avgRewardPes).toBe(0);
			// PES net is purely the cycle baseline: 50 * 0.05.
			expect(row.avgPesNet).toBeCloseTo(2.5, 12);
		}
	});
});

describe('computeQuestAnalytics: skill quests never blend into liquid', () => {
	const skillQuest = questRow({
		rewardPed: 5,
		rewardIsSkill: true,
		expectedRewardMarkupPercent: null,
		totalExpectedRewardPed: 10,
		linkedSessions: 4,
		totalWeaponCost: 200,
		totalHealCost: 0,
		totalEnhancerCost: 0,
		totalArmourCost: 0,
	});

	it('contributes zero liquid reward in BOTH display modes', () => {
		for (const mode of ['tt', 'markup'] as const) {
			const [row] = computeQuestAnalytics([skillQuest], RATES, mode);
			expect(row.displayLiquidReward).toBe(0);
			// Liquid net sees only the cycle: 50 * 0.9 - 50.
			expect(row.avgNet).toBeCloseTo(-5, 12);
			expect(row.returnRate).toBeCloseTo(0.9, 12);
		}
	});

	it('rides the PES side at face value, invariant to the toggle', () => {
		for (const mode of ['tt', 'markup'] as const) {
			const [row] = computeQuestAnalytics([skillQuest], RATES, mode);
			expect(row.avgRewardPes).toBe(5);
			// 50 * 0.05 cycle PES + 5 reward PES.
			expect(row.avgPesNet).toBeCloseTo(7.5, 12);
		}
	});
});

describe('computeQuestAnalytics: division guards', () => {
	it('yields zero rates and zero returnRate when nothing was cycled', () => {
		const [row] = computeQuestAnalytics(
			[
				questRow({
					totalWeaponCost: 0,
					totalHealCost: 0,
					totalEnhancerCost: 0,
					totalArmourCost: 0,
				}),
			],
			RATES,
			'tt',
		);
		expect(row.avgCycled).toBe(0);
		expect(row.avgRawReturns).toBe(0);
		expect(row.returnRate).toBe(0);
		// Net degenerates to the bare reward.
		expect(row.avgNet).toBeCloseTo(2, 12);
	});

	it('falls back to one session when linkedSessions is zero', () => {
		const [row] = computeQuestAnalytics([questRow({ linkedSessions: 0 })], RATES, 'tt');
		// totalCycled / 1, not division by zero.
		expect(row.avgCycled).toBeCloseTo(100, 12);
		// Face reward total is rewardPed * 0 sessions.
		expect(row.displayLiquidReward).toBe(0);
		expect(Number.isFinite(row.avgNet)).toBe(true);
	});
});

describe('computePlaylistAnalytics', () => {
	// Fixture per session (2 matched): immediate reward 5 (2 PES + 3 liquid),
	// bonus 3 (1 PES + 2 liquid); expected totals carry face value for the PES
	// portion and 130% markup on the liquid portions; 50 PED cycled.
	it('splits face-value liquid and PES portions in TT mode', () => {
		const [row] = computePlaylistAnalytics([playlistRow()], RATES, 'tt');
		expect(row.displayImmediateReward).toBeCloseTo(3, 12);
		expect(row.displayBonusReward).toBeCloseTo(2, 12);
		expect(row.avgImmediateSkillReward).toBeCloseTo(2, 12);
		expect(row.avgBonusSkillReward).toBeCloseTo(1, 12);
		// 45 cycle returns + 3 + 2 - 50 cycled.
		expect(row.avgNet).toBeCloseTo(0, 12);
		expect(row.returnRate).toBeCloseTo(1, 12);
		// 50 * 0.05 + 2 + 1: PES stays face value.
		expect(row.avgPesNet).toBeCloseTo(5.5, 12);
	});

	it('derives markup-mode liquid by subtracting the PES sums from the expected totals', () => {
		const [row] = computePlaylistAnalytics([playlistRow()], RATES, 'markup');
		// (11.8 - 4) / 2 and (7.2 - 2) / 2.
		expect(row.displayImmediateReward).toBeCloseTo(3.9, 12);
		expect(row.displayBonusReward).toBeCloseTo(2.6, 12);
		// PES sub-lines are invariant to the toggle.
		expect(row.avgImmediateSkillReward).toBeCloseTo(2, 12);
		expect(row.avgBonusSkillReward).toBeCloseTo(1, 12);
		// Blended markup over the face liquid total: 6.5 / 5.
		expect(row.rewardMarkupPercent).toBeCloseTo(130, 12);
	});

	it('reports a null markup percent when the liquid face total is zero', () => {
		const pureSkill = playlistRow({
			totalImmediateRewardPed: 4,
			totalBonusRewardPed: 2,
			totalImmediatePesReward: 4,
			totalBonusPesReward: 2,
			totalExpectedImmediateRewardPed: 4,
			totalExpectedBonusRewardPed: 2,
		});
		for (const mode of ['tt', 'markup'] as const) {
			const [row] = computePlaylistAnalytics([pureSkill], RATES, mode);
			expect(row.rewardMarkupPercent).toBeNull();
			// All-PES rewards leave the liquid displays at zero in both modes.
			expect(row.displayImmediateReward).toBeCloseTo(0, 12);
			expect(row.displayBonusReward).toBeCloseTo(0, 12);
		}
	});

	it('falls back to one session when matchedSessions is zero and guards zero cycled', () => {
		const [row] = computePlaylistAnalytics(
			[
				playlistRow({
					matchedSessions: 0,
					totalWeaponCost: 0,
					totalHealCost: 0,
					totalEnhancerCost: 0,
					totalArmourCost: 0,
				}),
			],
			RATES,
			'tt',
		);
		expect(row.avgCycled).toBe(0);
		expect(row.returnRate).toBe(0);
		expect(Number.isFinite(row.avgNet)).toBe(true);
	});
});
