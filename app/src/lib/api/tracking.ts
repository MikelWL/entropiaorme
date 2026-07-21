/**
 * The tracking family: session lifecycle, the consolidated snapshot,
 * session reads and post-hoc edits, mob locking, and the quest-link
 * flow. Thin wrappers over the generated typed commands; the session
 * and snapshot reads swap onto the parallel `demo_*` commands while the
 * guide is active (see `./guide`).
 */

import type {
	HarvestGuardrailAlert,
	MobEntryMode,
	MobSource,
	NotableEventCategory,
	NotableEventType,
	RecentEvent,
	TrackingSnapshot,
	TrackingState,
	TrifectaAttribution,
	WeaponAttribution,
} from './commands.gen';
import * as commands from './commands.gen';
import { guideSwapped } from './guide';

/** The snapshot shape the status-flavoured consumers (the stat
 * registry, the overlay pills) render from. */
export type TrackingStatus = TrackingSnapshot;

/** The overlay strip's camelCase render shape, mapped field by field
 * from the consolidated snapshot (`applySnapshot` in the overlay
 * route). Frontend-owned: this is a view model, not a wire shape. */
export interface TrackingLive {
	status: TrackingState;
	sessionId?: string | null;
	elapsed?: number | null;
	killCount?: number | null;
	kills?: number | null;
	cost?: number | null;
	returns?: number | null;
	pes?: number | null;
	net?: number | null;
	returnRate?: number | null;
	weaponAttribution?: WeaponAttribution | null;
	repairOcrEnabled?: boolean | null;
	endOfSessionArmourReminderEnabled?: boolean | null;
	mobEntryMode?: MobEntryMode | null;
	currentMob?: string | null;
	mobSource?: MobSource | null;
	currentTool?: string | null;
	trifectaAttribution?: TrifectaAttribution | null;
	harvestGuardrail?: HarvestGuardrailAlert | null;
	recentEvents?: {
		type: NotableEventCategory;
		eventType?: NotableEventType;
		description: string;
		value: number;
		timestamp?: string | number;
	}[];
}

export type { HarvestGuardrailAlert, RecentEvent, TrackingSnapshot };

export const startTracking = commands.trackingStart;
export const stopTracking = commands.trackingStop;
export const releaseMob = commands.trackingReleaseMob;
export const deleteSession = commands.trackingSessionDelete;
export const deactivateLootItem = commands.trackingLootItemDeactivate;
export const activateLootItem = commands.trackingLootItemActivate;
export const renameSessionMob = commands.trackingRenameMob;
export const restoreSessionMob = commands.trackingRestoreMob;
export const lockTrackingTag = commands.trackingTagLock;
export const scanRepairCost = commands.trackingRepairScan;
export const saveArmourCost = commands.trackingArmourCost;
export const getSessionQuestLinkSuggestion = commands.trackingQuestLinkSuggestion;

export const getTrackingSessions = guideSwapped(
	commands.trackingSessions,
	commands.demoTrackingSessions,
);
export const getSessionDetail = guideSwapped(
	commands.trackingSessionDetail,
	commands.demoTrackingSessionDetail,
);
export const getTrackingSnapshot = guideSwapped(
	commands.trackingSnapshot,
	commands.demoTrackingSnapshot,
);

export async function getTrackingTagSuggestions(query: string): Promise<string[]> {
	if (!query.trim()) return [];
	return commands.trackingTagSuggestions(query.trim(), null);
}

export async function getManualMobSuggestions(query: string) {
	if (!query.trim()) return [];
	return commands.trackingManualMobSuggestions(query.trim(), null);
}

export async function lockManualMob(species: string, maturity = '') {
	return commands.trackingManualMobLock(species, maturity);
}

export async function decideSessionQuestLink(sessionId: string, action: 'accept' | 'decline') {
	return commands.trackingQuestLink(sessionId, action);
}
