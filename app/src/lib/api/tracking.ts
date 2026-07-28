/**
 * The tracking family: session lifecycle, the consolidated snapshot,
 * session reads and post-hoc edits, mob locking, and the quest-link
 * flow. Thin wrappers over the generated typed commands; the session
 * and snapshot reads swap onto the parallel `demo_*` commands while the
 * guide is active (see `./guide`).
 */

import type {
	HarvestGuardrailAlert,
	NotableEventCategory,
	NotableEventType,
	RecentEvent,
	ToolActivity,
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
	sessionName?: string | null;
	skillBoostPercent?: number | null;
	/** The open segment's name (a segment exists only while active). */
	segmentName?: string | null;
	currentMob?: string | null;
	currentTool?: string | null;
	/** What the held tool implies the next action records as. */
	currentActivity?: ToolActivity | null;
	/** The open quest slices' names, newest first (auto-recorded by the
	 * quest lifecycle; several dailies can stack). */
	questNames?: string[] | null;
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
export const renameSession = commands.trackingRenameSession;
export const renameSessionMob = commands.trackingRenameMob;
export const restoreSessionMob = commands.trackingRestoreMob;
/** Set the session facets (full-state apply: null clears a facet). The
 * name is fixed while a session runs (the backend answers 409 on an
 * attempted change; correct it post-hoc via `renameSession`); the boost
 * stays editable throughout. */
export const setSessionConfig = commands.trackingSessionConfig;
export const scanRepairCost = commands.trackingRepairScan;
export const saveArmourCost = commands.trackingArmourCost;
/** Open a player-drawn segment on the running session, closing any
 * standing one; a null name is auto-numbered ("Segment N"). */
export const openSessionSegment = commands.trackingSegmentOpen;
export const closeSessionSegment = commands.trackingSegmentClose;
/** Rename the open segment live (its grain is finer than the session). */
export const renameSessionSegment = commands.trackingSegmentRename;

const readSessionsPage = guideSwapped(commands.trackingSessions, commands.demoTrackingSessions);

/** One keyset page of sessions plus the cursor for the next page (null on
 * the last page). */
export async function getTrackingSessions(cursor?: string, limit?: number) {
	return readSessionsPage(cursor ?? null, limit ?? null);
}
export const getSessionDetail = guideSwapped(
	commands.trackingSessionDetail,
	commands.demoTrackingSessionDetail,
);
export const getTrackingSnapshot = guideSwapped(
	commands.trackingSnapshot,
	commands.demoTrackingSnapshot,
);

/** Prior session names matching the query, most-used first: reusing a
 * name is what keeps the designated analytics axis grouping cleanly. */
export async function getSessionNameSuggestions(query: string): Promise<string[]> {
	if (!query.trim()) return [];
	return commands.trackingSessionNameSuggestions(query.trim(), null);
}

export async function getManualMobSuggestions(query: string) {
	if (!query.trim()) return [];
	return commands.trackingManualMobSuggestions(query.trim(), null);
}

export async function lockManualMob(species: string, maturity = '') {
	return commands.trackingManualMobLock(species, maturity);
}
