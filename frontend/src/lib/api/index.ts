/**
 * Backend API client: typed fetch wrappers for the Python backend.
 *
 * All backend communication goes through this module. Since the generated
 * client landed, each wrapper delegates to `client` (openapi-fetch over the
 * generated `schema.d.ts`), which verifies the path, method, parameters, and
 * request body against the backend's OpenAPI contract at compile time. The
 * wrappers keep their hand-written return types: those interfaces are the
 * authoritative frontend contract and may deliberately narrow the generated
 * schema (see `unwrap` in `./client`). The public surface of this module is
 * unchanged from its hand-rolled predecessor.
 */

export { ApiError, manualSkillScanCapturePng, request } from './client';

import { guideState } from '$lib/guide/state.svelte';
import type { NotableEventCategory, NotableEventType } from '$lib/types/common';
import { ApiError, client, unwrap } from './client';

/*
 * Guide-mode route swap for analytics-flavoured endpoints.
 *
 * When the interactive user guide is active on an analytics-backed surface
 * (analytics or dashboard), reads of analytics / tracking / ledger / inventory
 * are transparently retargeted onto the parallel `/api/demo/*` namespace
 * served by the curated demo DB. Surface components stay unchanged. Only the
 * read wrappers below branch on guide state, per call (never at client
 * construction); everything else (live tracking, mutating verbs, etc.) goes
 * to the real backend regardless of guide state.
 */

// --- Character stats ---

import type {
	CalibrationStatus,
	CharacterProspectOptions,
	CodexClaimResult,
	CodexMetaAttribute,
	CodexMetaClaimResult,
	CodexRankBreakdown,
	CodexSkillOption,
	CodexSpecies,
	ComputedCharacterStats,
	HpOptimizerResult,
	PathOptimizerResult,
	ProfessionLevel,
	ProfessionOptimizerResult,
	ProspectResult,
	SkillLevel,
} from '$lib/types/analytics';
import type { ProspectQuery } from './commands.gen';
import * as commands from './commands.gen';

// The character family is served over typed IPC commands
// (`commands.gen.ts`, generated from the Rust DTOs): the wrappers keep
// their signatures, with the hand-written `$lib/types/analytics`
// interfaces still the declared contract they narrow onto, exactly as
// `unwrap<T>` asserted before.

export async function getCalibrationStatus(): Promise<CalibrationStatus> {
	return (await commands.characterCalibration()) as CalibrationStatus;
}

export async function getCharacterStats(): Promise<ComputedCharacterStats> {
	return (await commands.characterStats()) as ComputedCharacterStats;
}

export async function getCharacterSkills(): Promise<SkillLevel[]> {
	return (await commands.characterSkills()) as SkillLevel[];
}

export async function getCharacterProfessions(): Promise<ProfessionLevel[]> {
	return (await commands.characterProfessions()) as ProfessionLevel[];
}

export async function getProfessionOptimizer(
	profession: string,
): Promise<ProfessionOptimizerResult> {
	return (await commands.characterProfessionOptimizer(profession)) as ProfessionOptimizerResult;
}

export async function getProfessionPathOptimizer(
	profession: string,
	params: { targetLevel: number } | { pedBudget: number },
): Promise<PathOptimizerResult> {
	const targetLevel = 'targetLevel' in params ? params.targetLevel : null;
	const pedBudget = 'pedBudget' in params ? params.pedBudget : null;
	return (await commands.characterPathOptimizer(
		profession,
		targetLevel,
		pedBudget,
	)) as PathOptimizerResult;
}

export async function getHpOptimizer(): Promise<HpOptimizerResult> {
	return (await commands.characterHpOptimizer()) as HpOptimizerResult;
}

export async function getCharacterProspectOptions(): Promise<CharacterProspectOptions> {
	return (await commands.characterProspectOptions()) as CharacterProspectOptions;
}

export async function getCharacterProspect(params: {
	profession: string;
	targetLevel: number;
	sliceType: 'global' | 'tag' | 'mob' | 'weapon';
	sliceValue?: string | null;
	markupUplift?: number;
}): Promise<ProspectResult> {
	const query: ProspectQuery = {
		profession: params.profession,
		targetLevel: params.targetLevel,
		sliceType: params.sliceType,
	};
	if (params.sliceType !== 'global' && params.sliceValue) {
		query.sliceValue = params.sliceValue;
	}
	if ((params.markupUplift ?? 0) > 0) {
		query.markupUplift = params.markupUplift;
	}
	return (await commands.characterProspect(query)) as ProspectResult;
}

// --- Manual scan flow (public, user-driven page-by-page capture) ---

export type ScanPhase = 'idle' | 'capturing' | 'processing' | 'awaiting_review';

export interface ScanManualStatus {
	active: boolean;
	processing: boolean;
	captured_pages: number;
	expected_pages: number;
	last_scan_time: number | null;
	skills_count?: number;
	configured: boolean;
	game_window_present: boolean;
	phase: ScanPhase;
	processing_progress: { done: number; total: number };
	has_pending_result: boolean;
	error: string | null;
}

export interface SkillScanPending {
	skills: Record<string, number>;
}

export async function getManualSkillScanStatus(): Promise<ScanManualStatus> {
	return unwrap(client.GET('/api/scan/skills/status'));
}

export async function startManualSkillScan(
	pageCount?: number,
): Promise<ScanManualStatus & { error?: string }> {
	return unwrap(
		client.POST('/api/scan/skills/start', { params: { query: { page_count: pageCount } } }),
	);
}

export async function captureManualSkillPage(): Promise<
	ScanManualStatus & { page?: number; captured?: boolean; error?: string }
> {
	return unwrap(client.POST('/api/scan/skills/capture'));
}

export async function cancelManualSkillScan(): Promise<ScanManualStatus & { error?: string }> {
	return unwrap(client.POST('/api/scan/skills/cancel'));
}

export async function undoManualSkillCapture(): Promise<
	ScanManualStatus & { undone_page?: number; error?: string }
> {
	return unwrap(client.POST('/api/scan/skills/undo'));
}

export async function processManualSkillScan(): Promise<ScanManualStatus & { error?: string }> {
	return unwrap(client.POST('/api/scan/skills/process'));
}

export async function acceptManualSkillScan(): Promise<{
	ok?: boolean;
	skills_persisted?: number;
	error?: string;
}> {
	return unwrap(client.POST('/api/scan/skills/accept'));
}

export async function rejectManualSkillScan(): Promise<{ ok?: boolean; error?: string }> {
	return unwrap(client.POST('/api/scan/skills/reject'));
}

export async function getManualSkillScanPending(): Promise<SkillScanPending | null> {
	try {
		return await unwrap<SkillScanPending>(client.GET('/api/scan/skills/pending'));
	} catch (err) {
		if (err instanceof ApiError && err.status === 404) return null;
		throw err;
	}
}

export async function setSpacebarCapture(
	enabled: boolean,
): Promise<{ ok?: boolean; enabled?: boolean; error?: string }> {
	return unwrap(client.POST('/api/scan/spacebar-capture', { params: { query: { enabled } } }));
}

// --- Codex ---
// Served over typed IPC commands (`commands.gen.ts`); the wrappers keep
// their hand-written return types (the authoritative frontend contract),
// narrowing the generated types with `as`.

export async function getCodexSpecies(): Promise<CodexSpecies[]> {
	return (await commands.codexSpecies()) as CodexSpecies[];
}

export async function getCodexSpeciesRanks(name: string): Promise<CodexRankBreakdown> {
	return (await commands.codexSpeciesRanks(name)) as CodexRankBreakdown;
}

export async function claimCodexRank(
	speciesName: string,
	rank: number,
	skillName: string,
): Promise<CodexClaimResult> {
	return (await commands.codexClaim(speciesName, rank, skillName)) as CodexClaimResult;
}

export async function unclaimCodexRank(speciesName: string): Promise<CodexClaimResult> {
	return (await commands.codexUnclaim(speciesName)) as CodexClaimResult;
}

export async function calibrateCodex(
	speciesName: string,
	rank: number,
): Promise<{ speciesName: string; rank: number }> {
	return (await commands.codexCalibrate(speciesName, rank)) as {
		speciesName: string;
		rank: number;
	};
}

export async function getCodexRecommendation(
	speciesName: string,
	rank: number,
	options?: { target?: 'profession' | 'hp'; profession?: string },
): Promise<CodexSkillOption[]> {
	return (await commands.codexRecommend(
		speciesName,
		rank,
		options?.profession ?? null,
		options?.target ?? 'profession',
	)) as CodexSkillOption[];
}

// --- Codex Meta ---

export async function getCodexMetaAttributes(): Promise<CodexMetaAttribute[]> {
	return (await commands.codexMetaAttributes()) as CodexMetaAttribute[];
}

export async function claimCodexMeta(attributeName: string): Promise<CodexMetaClaimResult> {
	return (await commands.codexMetaClaim(attributeName)) as CodexMetaClaimResult;
}

// --- Equipment ---
// The first family served over typed IPC commands (`commands.gen.ts`,
// generated from the Rust DTOs): the wrappers keep their signatures,
// with the hand-written `$lib/types` interfaces still the declared
// contract they narrow onto, exactly as `unwrap<T>` asserted before.

import type { Equipment, EquipmentDetail } from '$lib/types/equipment';
import type { EquipmentRequest, EquipmentSearchHit, SearchKind } from './commands.gen';

/** Search result from the equipment catalogue search command. The two
 * optional fields are not part of the wire shape: the equipment page
 * reuses this type to seed its selection state from a stored detail,
 * which carries them. */
export type EquipmentSearchResult = EquipmentSearchHit & {
	markupPercent?: number;
	damageEnhancers?: number;
};

type AddLibraryRequest = EquipmentRequest;

export async function searchEquipmentItems(
	q: string,
	type: SearchKind,
): Promise<EquipmentSearchResult[]> {
	if (q.length < 2) return [];
	return commands.equipmentSearch(q, type);
}

export async function getEquipmentLibrary(): Promise<Equipment[]> {
	return (await commands.equipmentLibrary()) as Equipment[];
}

export async function addToLibrary(req: AddLibraryRequest): Promise<Equipment> {
	return (await commands.equipmentAdd(req)) as Equipment;
}

export async function removeFromLibrary(id: string): Promise<void> {
	await commands.equipmentDelete(Number(id));
}

export async function updateLibrary(id: string, req: AddLibraryRequest): Promise<Equipment> {
	return (await commands.equipmentUpdate(Number(id), req)) as Equipment;
}

export async function getEquipmentDetail(id: string): Promise<EquipmentDetail> {
	return (await commands.equipmentDetail(Number(id))) as EquipmentDetail;
}

// --- Tracking ---

import type { SessionDetail, TrackingSession } from '$lib/types/tracking';

export interface TrackingStatus {
	status: 'unavailable' | 'idle' | 'active';
	session_id?: string;
	started_at?: string;
	kill_count?: number;
	cost?: number;
	returns?: number;
	pes?: number;
	returnRate?: number;
	damageDealtTotal?: number;
	weaponDamageDealt?: number;
	weaponCost?: number;
	shotsFiredTotal?: number;
	criticalHitsTotal?: number;
	maxDamage?: number;
	globalsCount?: number;
	hofsCount?: number;
	latestKillLoot?: number | null;
	multiplierLast?: number | null;
	multiplierAvg?: number | null;
	multiplierMax?: number | null;
	multiplierHistory?: number[];
	cumulativeNetHistory?: number[];
	hotbarListenerActive?: boolean;
	weaponAttribution?: 'hotbar' | 'trifecta';
	repairOcrEnabled?: boolean;
	endOfSessionArmourReminderEnabled?: boolean;
	mobEntryMode?: 'mob' | 'tag';
	currentMob?: string | null;
	mobSource?: 'manual' | 'tag' | null;
}

export interface RecentEvent {
	id: string;
	type: NotableEventCategory;
	eventType: NotableEventType;
	description: string;
	value: number | null;
	timestamp: string;
}

export async function startTracking(): Promise<{
	session_id: string;
	started_at: string;
	status: string;
}> {
	return unwrap(client.POST('/api/tracking/start'));
}

export async function stopTracking(): Promise<{ session_id: string; kill_count: number }> {
	return unwrap(client.POST('/api/tracking/stop'));
}

export async function getTrackingSessions(): Promise<TrackingSession[]> {
	return unwrap(
		guideState.isActive
			? client.GET('/api/demo/tracking/sessions')
			: client.GET('/api/tracking/sessions'),
	);
}

export async function getSessionDetail(sessionId: string): Promise<SessionDetail> {
	const params = { path: { session_id: sessionId } };
	return unwrap(
		guideState.isActive
			? client.GET('/api/demo/tracking/session/{session_id}', { params })
			: client.GET('/api/tracking/session/{session_id}', { params }),
	);
}

export async function deleteSession(sessionId: string): Promise<void> {
	await client.DELETE('/api/tracking/session/{session_id}', {
		params: { path: { session_id: sessionId } },
	});
}

/** Response shape from the loot-item deactivate / activate endpoints.
 * Wholesale-by-item-name: flips every kill_loot_items row matching
 * `(sessionId, itemName)` in one atomic transaction. */
export interface LootItemEditResponse {
	sessionId: string;
	itemName: string;
	affectedRows: number;
	totalValueDelta: number;
	sessionTotalReturns: number;
}

export async function deactivateLootItem(
	sessionId: string,
	itemName: string,
): Promise<LootItemEditResponse> {
	return unwrap(
		client.POST('/api/tracking/session/{session_id}/loot-item/{item_name}/deactivate', {
			params: { path: { session_id: sessionId, item_name: itemName } },
		}),
	);
}

export async function activateLootItem(
	sessionId: string,
	itemName: string,
): Promise<LootItemEditResponse> {
	return unwrap(
		client.POST('/api/tracking/session/{session_id}/loot-item/{item_name}/activate', {
			params: { path: { session_id: sessionId, item_name: itemName } },
		}),
	);
}

/** Response shape from the rename-mob / restore-mob endpoints. */
export interface MobEditResponse {
	sessionId: string;
	mobName: string;
	killCount: number;
}

export async function renameSessionMob(
	sessionId: string,
	fromMobName: string,
	toMobName: string,
): Promise<MobEditResponse> {
	return unwrap(
		client.POST('/api/tracking/session/{session_id}/rename-mob', {
			params: { path: { session_id: sessionId } },
			body: { fromMobName, toMobName },
		}),
	);
}

export async function restoreSessionMob(
	sessionId: string,
	currentMobName: string,
): Promise<MobEditResponse> {
	return unwrap(
		client.POST('/api/tracking/session/{session_id}/restore-mob', {
			params: { path: { session_id: sessionId } },
			body: { currentMobName },
		}),
	);
}

export interface TrackingLive {
	status: 'unavailable' | 'idle' | 'active';
	sessionId?: string;
	elapsed?: number;
	killCount?: number;
	kills?: number;
	cost?: number;
	returns?: number;
	pes?: number;
	net?: number;
	returnRate?: number;
	weaponAttribution?: 'hotbar' | 'trifecta';
	repairOcrEnabled?: boolean;
	endOfSessionArmourReminderEnabled?: boolean;
	mobEntryMode?: 'mob' | 'tag';
	currentMob?: string | null;
	mobSource?: 'manual' | 'tag' | null;
	currentTool?: string | null;
	trifectaAttribution?: {
		activePresetId: string | null;
		presetName: string | null;
		presets: {
			id: string;
			name: string;
		}[];
		smallWeapon: string | null;
		bigWeapon: string | null;
		healTool: string | null;
	} | null;
	recentEvents?: {
		type: NotableEventCategory | 'warning';
		eventType?: NotableEventType;
		description: string;
		value: number;
		timestamp?: string | number;
	}[];
}

/**
 * The consolidated tracking readout: one hydration-only endpoint that unions the
 * legacy status, live, and recent-events shapes (the polled trio it replaces).
 * The dashboard reads its render shape from here and re-reads it on a backend
 * tracking event, rather than polling the three endpoints.
 *
 * Shape is the status superset (snake `session_id` / `started_at` / `kill_count`,
 * camelCase headline numbers, the shared config fields) plus the live-only
 * `elapsed` / `net` / `currentTool` / `trifectaAttribution`, the `recentEvents`
 * activity feed, and a `warnings` sibling array. Active-only fields are absent
 * when idle, where `recentEvents` is `[]` (the feed clears on idle).
 */
export interface TrackingSnapshot extends TrackingStatus {
	elapsed?: number;
	net?: number;
	currentTool?: string | null;
	trifectaAttribution?: TrackingLive['trifectaAttribution'];
	recentEvents?: RecentEvent[];
	warnings?: { type: 'warning'; description: string; value: number }[];
}

export async function getTrackingSnapshot(): Promise<TrackingSnapshot> {
	return unwrap(
		guideState.isActive
			? client.GET('/api/demo/tracking/snapshot')
			: client.GET('/api/tracking/snapshot'),
	);
}

export async function releaseMob(): Promise<{ released: string | null }> {
	return unwrap(client.POST('/api/tracking/release-mob'));
}

export interface ManualMobSuggestion {
	display: string;
	species: string;
	maturity: string;
}

export async function getTrackingTagSuggestions(query: string): Promise<string[]> {
	if (!query.trim()) return [];
	return unwrap(
		client.GET('/api/tracking/tag-suggestions', { params: { query: { q: query.trim() } } }),
	);
}

export async function lockTrackingTag(tag: string): Promise<{ tag: string }> {
	return unwrap(client.POST('/api/tracking/tag-lock', { body: { tag } }));
}

export async function getManualMobSuggestions(query: string): Promise<ManualMobSuggestion[]> {
	if (!query.trim()) return [];
	return unwrap(
		client.GET('/api/tracking/manual-mob-suggestions', { params: { query: { q: query.trim() } } }),
	);
}

export async function lockManualMob(
	species: string,
	maturity = '',
): Promise<{
	mobName: string;
	species: string;
	maturity: string;
}> {
	return unwrap(client.POST('/api/tracking/manual-mob-lock', { body: { species, maturity } }));
}

export async function scanRepairCost(
	sessionId: string,
): Promise<{ cost_ped: number; raw_text: string; confidence: number; error?: string }> {
	return unwrap(
		client.POST('/api/tracking/session/{session_id}/repair-scan', {
			params: { path: { session_id: sessionId } },
		}),
	);
}

export async function saveArmourCost(
	sessionId: string,
	cost: number,
): Promise<{ sessionId: string; armourCost: number }> {
	return unwrap(
		client.POST('/api/tracking/session/{session_id}/armour-cost', {
			params: { path: { session_id: sessionId } },
			body: { cost },
		}),
	);
}

export interface SessionQuestLinkSuggestion {
	sessionId: string;
	suggestionType: 'quest' | 'playlist' | 'none';
	reason:
		| 'single_quest'
		| 'exact_playlist'
		| 'no_completions'
		| 'unclean'
		| 'ambiguous_playlist'
		| 'declined'
		| 'already_linked';
	questId: string | null;
	questName: string | null;
	playlistId: string | null;
	playlistName: string | null;
}

export interface SessionQuestLinkDecision {
	sessionId: string;
	status: 'linked' | 'declined';
	linkType?: 'quest' | 'playlist';
	questId?: string | null;
	questName?: string | null;
	playlistId?: string | null;
	playlistName?: string | null;
}

export async function getSessionQuestLinkSuggestion(
	sessionId: string,
): Promise<SessionQuestLinkSuggestion> {
	return unwrap(
		client.GET('/api/tracking/session/{session_id}/quest-link-suggestion', {
			params: { path: { session_id: sessionId } },
		}),
	);
}

export async function decideSessionQuestLink(
	sessionId: string,
	action: 'accept' | 'decline',
): Promise<SessionQuestLinkDecision> {
	return unwrap(
		client.POST('/api/tracking/session/{session_id}/quest-link', {
			params: { path: { session_id: sessionId } },
			body: { action },
		}),
	);
}

// --- Analytics ---

import type {
	InventoryItem,
	InventorySellResult,
	LedgerEntry,
	LedgerPreset,
	MobComparison,
	OverviewStats,
	TagComparison,
	WeaponComparison,
} from '$lib/types/analytics';

export interface ActivityData {
	mobComparisons: MobComparison[];
	tagComparisons: TagComparison[];
	weaponComparisons: WeaponComparison[];
}

// The live analytics surface is served over typed IPC commands
// (`commands.gen.ts`); the wrappers keep their hand-written return types,
// narrowing the generated shapes with `as` (the `unwrap<T>` doctrine's
// typed-IPC form). Guide mode still reads the parallel `/api/demo/*`
// namespace over the curated demo database (its own migration is pending),
// so the read wrappers keep branching on guide state.
export async function getAnalyticsOverview(period: string = 'all'): Promise<OverviewStats> {
	if (guideState.isActive) {
		return unwrap(client.GET('/api/demo/analytics/overview', { params: { query: { period } } }));
	}
	return (await commands.analyticsOverview(period)) as OverviewStats;
}

export async function getAnalyticsActivity(): Promise<ActivityData> {
	if (guideState.isActive) {
		return unwrap(client.GET('/api/demo/analytics/activity'));
	}
	return (await commands.analyticsActivity()) as ActivityData;
}

/** One keyset page of ledger entries plus the cursor for the next page
 * (null on the last page). */
export interface LedgerPage {
	items: LedgerEntry[];
	nextCursor: string | null;
}

export async function getLedgerEntries(cursor?: string, limit?: number): Promise<LedgerPage> {
	if (guideState.isActive) {
		const query = {
			...(cursor ? { cursor } : {}),
			...(limit != null ? { limit } : {}),
		};
		const { data, response } = await client.GET('/api/demo/analytics/ledger', {
			params: { query },
		});
		return {
			items: (data ?? []) as LedgerEntry[],
			nextCursor: response.headers.get('x-next-cursor'),
		};
	}
	const page = await commands.ledgerList(cursor ?? null, limit ?? null);
	return {
		items: page.entries as LedgerEntry[],
		nextCursor: page.nextCursor ?? null,
	};
}

export async function addLedgerEntry(entry: Omit<LedgerEntry, 'id'>): Promise<LedgerEntry> {
	return (await commands.ledgerCreate(entry as commands.LedgerEntryInput)) as LedgerEntry;
}

export async function deleteLedgerEntry(id: string): Promise<void> {
	await commands.ledgerDelete(id);
}

export async function getLedgerPresets(): Promise<LedgerPreset[]> {
	if (guideState.isActive) {
		return unwrap(client.GET('/api/demo/analytics/ledger/presets'));
	}
	return (await commands.ledgerPresetsList()) as LedgerPreset[];
}

export async function addLedgerPreset(preset: Omit<LedgerPreset, 'id'>): Promise<LedgerPreset> {
	return (await commands.ledgerPresetCreate(preset as commands.LedgerPresetInput)) as LedgerPreset;
}

export async function deleteLedgerPreset(id: string): Promise<void> {
	await commands.ledgerPresetDelete(id);
}

// --- Inventory Ledger ---

export interface InventoryItemPayload {
	name: string;
	tt_value: number;
	markup_paid: number;
	notes?: string | null;
	acquired_at?: string;
}

export interface InventoryItemPatchPayload {
	name?: string;
	tt_value?: number;
	markup_paid?: number;
	notes?: string | null;
}

export interface InventorySellPayload {
	sale_price: number;
	description?: string;
	sold_at?: string;
}

export async function getInventoryItems(): Promise<InventoryItem[]> {
	if (guideState.isActive) {
		return unwrap(client.GET('/api/demo/analytics/inventory'));
	}
	return (await commands.inventoryList()) as InventoryItem[];
}

export async function addInventoryItem(payload: InventoryItemPayload): Promise<InventoryItem> {
	return (await commands.inventoryCreate(payload as commands.InventoryItemInput)) as InventoryItem;
}

export async function updateInventoryItem(
	id: string,
	patch: InventoryItemPatchPayload,
): Promise<InventoryItem> {
	return (await commands.inventoryUpdate(id, patch as commands.InventoryPatch)) as InventoryItem;
}

export async function deleteInventoryItem(id: string): Promise<void> {
	await commands.inventoryDelete(id);
}

export async function sellInventoryItem(
	id: string,
	payload: InventorySellPayload,
): Promise<InventorySellResult> {
	return (await commands.inventorySell(
		id,
		payload as commands.InventorySellInput,
	)) as InventorySellResult;
}

// --- Quests ---

import type {
	PlaylistAnalyticsRow,
	PlaylistCreateData,
	PlaylistUpdateData,
	Quest,
	QuestAnalyticsRow,
	QuestCreateData,
	QuestPlaylist,
	QuestUpdateData,
} from '$lib/types/quests';

// Served over typed IPC commands (`commands.gen.ts`); the wrappers keep
// their string-id signatures and hand-written return types, narrowing the
// generated shapes with `as` (the `unwrap<T>` doctrine's typed-IPC form).
export async function getQuests(): Promise<Quest[]> {
	return (await commands.questsList()) as Quest[];
}

export async function getQuest(id: string): Promise<Quest> {
	return (await commands.questGet(Number(id))) as Quest;
}

export async function createQuest(data: QuestCreateData): Promise<Quest> {
	return (await commands.questCreate(data as commands.QuestInput)) as Quest;
}

export async function updateQuest(id: string, data: QuestUpdateData): Promise<Quest> {
	return (await commands.questUpdate(Number(id), data as commands.QuestInput)) as Quest;
}

export async function deleteQuest(id: string): Promise<void> {
	await commands.questDelete(Number(id));
}

export async function startQuest(id: string): Promise<Quest> {
	return (await commands.questStart(Number(id))) as Quest;
}

export async function completeQuest(id: string): Promise<Quest> {
	return (await commands.questComplete(Number(id))) as Quest;
}

export async function cancelQuest(id: string, undoReward = false): Promise<Quest> {
	return (await commands.questCancel(Number(id), undoReward)) as Quest;
}

export async function getQuestAnalytics(): Promise<QuestAnalyticsRow[]> {
	return (await commands.questsAnalytics()) as QuestAnalyticsRow[];
}

export async function getPlaylistAnalytics(): Promise<PlaylistAnalyticsRow[]> {
	return (await commands.playlistsAnalytics()) as PlaylistAnalyticsRow[];
}

export async function getPlaylists(): Promise<QuestPlaylist[]> {
	return (await commands.playlistsList()) as QuestPlaylist[];
}

export async function createPlaylist(data: PlaylistCreateData): Promise<QuestPlaylist> {
	return (await commands.playlistCreate(data as commands.PlaylistInput)) as QuestPlaylist;
}

export async function updatePlaylist(id: string, data: PlaylistUpdateData): Promise<QuestPlaylist> {
	return (await commands.playlistUpdate(
		Number(id),
		data as commands.PlaylistInput,
	)) as QuestPlaylist;
}

export async function deletePlaylist(id: string): Promise<void> {
	await commands.playlistDelete(Number(id));
}

// --- Settings ---

import type { AppSettings } from '$lib/types/settings';

export interface SettingsUpdate {
	chatlog_path?: string;
	player_name?: string;
	hotbar_hooks_enabled?: boolean;
	repair_ocr_enabled?: boolean;
	end_of_session_armour_reminder_enabled?: boolean;
	developer_mode_enabled?: boolean;
	mob_tracking_mode?: 'mob' | 'tag';
	mob_tracking_tag?: string;
	hotbar?: Record<string, number | null>;
	active_trifecta_preset_id?: string | null;
	trifecta_presets?: {
		id: string;
		name: string;
		small_weapon_id: number | null;
		big_weapon_id: number | null;
		heal_id: number | null;
	}[];
	loot_filter_blacklist?: string[];
}

export async function getSettings(): Promise<AppSettings> {
	return (await commands.settingsGet()) as AppSettings;
}

export async function updateSettings(updates: SettingsUpdate): Promise<AppSettings> {
	return (await commands.settingsUpdate(updates)) as AppSettings;
}

// --- Overlay ---

export async function getOverlayPosition(): Promise<{ x: number | null; y: number | null }> {
	return (await commands.settingsOverlayPosition()) as { x: number | null; y: number | null };
}

export async function saveOverlayPosition(x: number, y: number): Promise<void> {
	await commands.settingsSetOverlayPosition(x, y);
}
