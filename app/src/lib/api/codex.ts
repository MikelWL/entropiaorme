/**
 * The codex family: species, rank breakdowns, claims, calibration, the
 * reward recommendation, and the meta attributes. Thin wrappers over
 * the generated typed commands; argument shaping only.
 */

import type { CodexRecommendTarget, CodexSkillOption } from './commands.gen';
import * as commands from './commands.gen';

export const getCodexSpecies = commands.codexSpecies;
export const getCodexSpeciesRanks = commands.codexSpeciesRanks;
export const claimCodexRank = commands.codexClaim;
export const unclaimCodexRank = commands.codexUnclaim;
export const calibrateCodex = commands.codexCalibrate;
export const getCodexMetaAttributes = commands.codexMetaAttributes;
export const claimCodexMeta = commands.codexMetaClaim;
export const claimCodexMastery = commands.codexMasteryClaim;
export const unclaimCodexMastery = commands.codexMasteryUnclaim;

export async function getCodexMasteryOptions(options?: {
	target?: CodexRecommendTarget;
	profession?: string;
}): Promise<CodexSkillOption[]> {
	return commands.codexMasteryOptions(options?.profession ?? null, options?.target ?? 'profession');
}

export async function getCodexRecommendation(
	speciesName: string,
	rank: number,
	options?: { target?: CodexRecommendTarget; profession?: string },
): Promise<CodexSkillOption[]> {
	return commands.codexRecommend(
		speciesName,
		rank,
		options?.profession ?? null,
		options?.target ?? 'profession',
	);
}
