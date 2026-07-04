import { beforeEach, describe, expect, it, vi } from 'vitest';

// The facade's behaviour is its mapping: which client verb, which path, which
// params/body shape, and (for the analytics-flavoured reads) the per-call
// guide-mode demo dispatch. The generated client is mocked out wholesale, so
// these tests pin the facade layer alone; client.ts has its own suite.
// vi.hoisted: the module under test is imported statically, so the vi.mock
// factories run before ordinary top-level consts initialise; these seams must
// be hoisted alongside them.
const {
	clientGet,
	clientPost,
	clientPut,
	clientPatch,
	clientDelete,
	FakeApiError,
	guideState,
	tauriInvoke,
} = vi.hoisted(() => {
	class FakeApiError extends Error {
		constructor(
			public status: number,
			message: string,
		) {
			super(message);
			this.name = 'ApiError';
		}
	}
	return {
		tauriInvoke: vi.fn(),
		clientGet: vi.fn(),
		clientPost: vi.fn(),
		clientPut: vi.fn(),
		clientPatch: vi.fn(),
		clientDelete: vi.fn(),
		FakeApiError,
		// Mutable guide-state seam: tests flip isActive to drive demo dispatch.
		guideState: { isActive: false },
	};
});

vi.mock('./client', () => ({
	ApiError: FakeApiError,
	manualSkillScanCapturePng: async (page: number) => `data:image/png;base64,page${page}`,
	request: vi.fn(),
	unwrap: async (call: Promise<{ data?: unknown }>) => (await call).data,
	client: {
		GET: (...args: unknown[]) => clientGet(...args),
		POST: (...args: unknown[]) => clientPost(...args),
		PUT: (...args: unknown[]) => clientPut(...args),
		PATCH: (...args: unknown[]) => clientPatch(...args),
		DELETE: (...args: unknown[]) => clientDelete(...args),
	},
}));

vi.mock('$lib/guide/state.svelte', () => ({ guideState }));

// The typed-command transport under the generated bindings: the equipment
// family invokes Tauri commands rather than the HTTP-shaped client.
vi.mock('@tauri-apps/api/core', () => ({
	invoke: (...args: unknown[]) => tauriInvoke(...args),
}));

import * as api from './index';

const DATA = { marker: 'payload' } as const;
// GET results also carry a `response` (the raw Response) so header-reading
// callers like `getLedgerEntries` (which reads the X-Next-Cursor pagination
// header) have one; `unwrap`-based callers ignore it.
const GET_RESULT = { data: DATA, response: { headers: new Headers() } };

beforeEach(() => {
	guideState.isActive = false;
	for (const mock of [clientPost, clientPut, clientPatch, clientDelete]) {
		mock.mockReset();
		mock.mockResolvedValue({ data: DATA });
	}
	clientGet.mockReset();
	clientGet.mockResolvedValue(GET_RESULT);
	tauriInvoke.mockReset();
	tauriInvoke.mockResolvedValue(DATA);
});

type Verb = 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE';
const verbMock: Record<Verb, ReturnType<typeof vi.fn>> = {
	GET: clientGet,
	POST: clientPost,
	PUT: clientPut,
	PATCH: clientPatch,
	DELETE: clientDelete,
};

describe('plain delegating wrappers map to the expected verb, path, and shape', () => {
	const rows: [string, () => Promise<unknown>, Verb, string, unknown?][] = [
		[
			'getManualSkillScanStatus',
			() => api.getManualSkillScanStatus(),
			'GET',
			'/api/scan/skills/status',
		],
		[
			'startManualSkillScan',
			() => api.startManualSkillScan(5),
			'POST',
			'/api/scan/skills/start',
			{ params: { query: { page_count: 5 } } },
		],
		[
			'captureManualSkillPage',
			() => api.captureManualSkillPage(),
			'POST',
			'/api/scan/skills/capture',
		],
		['cancelManualSkillScan', () => api.cancelManualSkillScan(), 'POST', '/api/scan/skills/cancel'],
		['undoManualSkillCapture', () => api.undoManualSkillCapture(), 'POST', '/api/scan/skills/undo'],
		[
			'processManualSkillScan',
			() => api.processManualSkillScan(),
			'POST',
			'/api/scan/skills/process',
		],
		['acceptManualSkillScan', () => api.acceptManualSkillScan(), 'POST', '/api/scan/skills/accept'],
		['rejectManualSkillScan', () => api.rejectManualSkillScan(), 'POST', '/api/scan/skills/reject'],
		[
			'setSpacebarCapture',
			() => api.setSpacebarCapture(true),
			'POST',
			'/api/scan/spacebar-capture',
			{ params: { query: { enabled: true } } },
		],
		['startTracking', () => api.startTracking(), 'POST', '/api/tracking/start'],
		['stopTracking', () => api.stopTracking(), 'POST', '/api/tracking/stop'],
		[
			'deactivateLootItem',
			() => api.deactivateLootItem('s1', 'Shrapnel'),
			'POST',
			'/api/tracking/session/{session_id}/loot-item/{item_name}/deactivate',
			{ params: { path: { session_id: 's1', item_name: 'Shrapnel' } } },
		],
		[
			'activateLootItem',
			() => api.activateLootItem('s1', 'Shrapnel'),
			'POST',
			'/api/tracking/session/{session_id}/loot-item/{item_name}/activate',
			{ params: { path: { session_id: 's1', item_name: 'Shrapnel' } } },
		],
		[
			'renameSessionMob',
			() => api.renameSessionMob('s1', 'Atrox Young', 'Atrox Mature'),
			'POST',
			'/api/tracking/session/{session_id}/rename-mob',
			{
				params: { path: { session_id: 's1' } },
				body: { fromMobName: 'Atrox Young', toMobName: 'Atrox Mature' },
			},
		],
		[
			'restoreSessionMob',
			() => api.restoreSessionMob('s1', 'Atrox Mature'),
			'POST',
			'/api/tracking/session/{session_id}/restore-mob',
			{ params: { path: { session_id: 's1' } }, body: { currentMobName: 'Atrox Mature' } },
		],
		['releaseMob', () => api.releaseMob(), 'POST', '/api/tracking/release-mob'],
		[
			'lockTrackingTag',
			() => api.lockTrackingTag('team hunt'),
			'POST',
			'/api/tracking/tag-lock',
			{ body: { tag: 'team hunt' } },
		],
		[
			'lockManualMob defaults maturity to an empty string',
			() => api.lockManualMob('Atrox'),
			'POST',
			'/api/tracking/manual-mob-lock',
			{ body: { species: 'Atrox', maturity: '' } },
		],
		[
			'scanRepairCost',
			() => api.scanRepairCost('s1'),
			'POST',
			'/api/tracking/session/{session_id}/repair-scan',
			{ params: { path: { session_id: 's1' } } },
		],
		[
			'saveArmourCost',
			() => api.saveArmourCost('s1', 1.25),
			'POST',
			'/api/tracking/session/{session_id}/armour-cost',
			{ params: { path: { session_id: 's1' } }, body: { cost: 1.25 } },
		],
		[
			'getSessionQuestLinkSuggestion',
			() => api.getSessionQuestLinkSuggestion('s1'),
			'GET',
			'/api/tracking/session/{session_id}/quest-link-suggestion',
			{ params: { path: { session_id: 's1' } } },
		],
		[
			'decideSessionQuestLink',
			() => api.decideSessionQuestLink('s1', 'accept'),
			'POST',
			'/api/tracking/session/{session_id}/quest-link',
			{ params: { path: { session_id: 's1' } }, body: { action: 'accept' } },
		],
	];

	it.each(rows)('%s', async (_name, call, verb, path, options) => {
		const result = await call();
		const mock = verbMock[verb];
		expect(mock).toHaveBeenCalledTimes(1);
		if (options === undefined) {
			expect(mock).toHaveBeenCalledWith(path);
		} else {
			expect(mock).toHaveBeenCalledWith(path, options);
		}
		expect(result).toEqual(DATA);
	});
});

describe('void-returning wrappers delegate without unwrapping', () => {
	const rows: [string, () => Promise<void>, Verb, string, unknown][] = [
		[
			'deleteSession',
			() => api.deleteSession('s1'),
			'DELETE',
			'/api/tracking/session/{session_id}',
			{ params: { path: { session_id: 's1' } } },
		],
	];

	it.each(rows)('%s', async (_name, call, verb, path, options) => {
		await expect(call()).resolves.toBeUndefined();
		expect(verbMock[verb]).toHaveBeenCalledWith(path, options);
	});
});

describe('guide-mode demo dispatch', () => {
	const rows: [string, () => Promise<unknown>, string, string, unknown?][] = [
		[
			'getTrackingSessions',
			() => api.getTrackingSessions(),
			'/api/tracking/sessions',
			'/api/demo/tracking/sessions',
		],
		[
			'getSessionDetail',
			() => api.getSessionDetail('s1'),
			'/api/tracking/session/{session_id}',
			'/api/demo/tracking/session/{session_id}',
			{ params: { path: { session_id: 's1' } } },
		],
		[
			'getTrackingSnapshot',
			() => api.getTrackingSnapshot(),
			'/api/tracking/snapshot',
			'/api/demo/tracking/snapshot',
		],
	];

	it.each(
		rows,
	)('%s reads the real route normally and the demo route in guide mode', async (_name, call, realPath, demoPath, options) => {
		guideState.isActive = false;
		await call();
		expect(clientGet).toHaveBeenCalledTimes(1);
		expect(clientGet.mock.calls[0][0]).toBe(realPath);

		clientGet.mockClear();
		clientGet.mockResolvedValue(GET_RESULT);
		guideState.isActive = true;
		await call();
		expect(clientGet).toHaveBeenCalledTimes(1);
		expect(clientGet.mock.calls[0][0]).toBe(demoPath);
		if (options !== undefined) {
			expect(clientGet.mock.calls[0][1]).toEqual(options);
		}
	});
});

// The analytics family serves its live surface over typed IPC commands and
// keeps a per-call demo-route branch (the guide-mode surface still reads the
// `/api/demo/*` namespace until its own migration): the reads dispatch a
// command live and the HTTP client in guide mode; the writes are always
// commands (no demo branch).
describe('analytics wrappers dispatch typed commands', () => {
	it('getAnalyticsOverview invokes the command live and reads the demo route in guide mode', async () => {
		guideState.isActive = false;
		await api.getAnalyticsOverview('30d');
		expect(tauriInvoke).toHaveBeenCalledWith('analytics_overview', { period: '30d' });
		expect(clientGet).not.toHaveBeenCalled();

		tauriInvoke.mockClear();
		guideState.isActive = true;
		await api.getAnalyticsOverview('30d');
		expect(clientGet).toHaveBeenCalledWith('/api/demo/analytics/overview', {
			params: { query: { period: '30d' } },
		});
		expect(tauriInvoke).not.toHaveBeenCalled();
	});

	it('getAnalyticsOverview defaults the period to "all"', async () => {
		await api.getAnalyticsOverview();
		expect(tauriInvoke).toHaveBeenCalledWith('analytics_overview', { period: 'all' });
	});

	it('getAnalyticsActivity invokes the command live', async () => {
		await api.getAnalyticsActivity();
		expect(tauriInvoke).toHaveBeenCalledWith('analytics_activity', {});
	});

	it('getLedgerEntries invokes ledger_list and reshapes the page (cursor from the body)', async () => {
		tauriInvoke.mockResolvedValue({ entries: [{ id: 'e1' }], nextCursor: 'cur' });
		const page = await api.getLedgerEntries('seek', 25);
		expect(tauriInvoke).toHaveBeenCalledWith('ledger_list', { cursor: 'seek', limit: 25 });
		expect(page).toEqual({ items: [{ id: 'e1' }], nextCursor: 'cur' });
	});

	it('getLedgerEntries passes nulls for an unpaged first read', async () => {
		tauriInvoke.mockResolvedValue({ entries: [], nextCursor: null });
		const page = await api.getLedgerEntries();
		expect(tauriInvoke).toHaveBeenCalledWith('ledger_list', { cursor: null, limit: null });
		expect(page.nextCursor).toBeNull();
	});

	it('getLedgerPresets / getInventoryItems invoke their list commands live', async () => {
		await api.getLedgerPresets();
		expect(tauriInvoke).toHaveBeenCalledWith('ledger_presets_list', {});
		tauriInvoke.mockClear();
		await api.getInventoryItems();
		expect(tauriInvoke).toHaveBeenCalledWith('inventory_list', {});
	});

	it('the write wrappers invoke their commands (no demo branch, even in guide mode)', async () => {
		guideState.isActive = true;
		const entry = { date: '2026-05-01', type: 'expense', description: 'ammo', amount: 1, tag: 't' };
		await api.addLedgerEntry(entry as never);
		expect(tauriInvoke).toHaveBeenCalledWith('ledger_create', { entry });

		const preset = { name: 'resupply', type: 'expense', description: 'd', amount: 1, tag: 't' };
		await api.addLedgerPreset(preset as never);
		expect(tauriInvoke).toHaveBeenCalledWith('ledger_preset_create', { preset });

		const item = { name: 'ESI', tt_value: 10, markup_paid: 2 };
		await api.addInventoryItem(item);
		expect(tauriInvoke).toHaveBeenCalledWith('inventory_create', { item });

		await api.updateInventoryItem('i1', { tt_value: 12 });
		expect(tauriInvoke).toHaveBeenCalledWith('inventory_update', {
			item_id: 'i1',
			patch: { tt_value: 12 },
		});

		await api.sellInventoryItem('i1', { sale_price: 15 });
		expect(tauriInvoke).toHaveBeenCalledWith('inventory_sell', {
			item_id: 'i1',
			sale: { sale_price: 15 },
		});

		expect(clientGet).not.toHaveBeenCalled();
		expect(clientPost).not.toHaveBeenCalled();
	});

	it('the delete wrappers invoke their commands and resolve void', async () => {
		tauriInvoke.mockResolvedValue(undefined);
		await expect(api.deleteLedgerEntry('e1')).resolves.toBeUndefined();
		expect(tauriInvoke).toHaveBeenCalledWith('ledger_delete', { entry_id: 'e1' });
		await expect(api.deleteLedgerPreset('p1')).resolves.toBeUndefined();
		expect(tauriInvoke).toHaveBeenCalledWith('ledger_preset_delete', { preset_id: 'p1' });
		await expect(api.deleteInventoryItem('i1')).resolves.toBeUndefined();
		expect(tauriInvoke).toHaveBeenCalledWith('inventory_delete', { item_id: 'i1' });
	});
});

describe('equipment wrappers dispatch typed commands', () => {
	it('searchEquipmentItems short-circuits to [] without a command below two characters', async () => {
		await expect(api.searchEquipmentItems('a', 'weapon')).resolves.toEqual([]);
		await expect(api.searchEquipmentItems('', 'weapon')).resolves.toEqual([]);
		expect(tauriInvoke).not.toHaveBeenCalled();
	});

	it('searchEquipmentItems invokes with q and kind from two characters', async () => {
		tauriInvoke.mockResolvedValue([]);
		await api.searchEquipmentItems('op', 'amp');
		expect(tauriInvoke).toHaveBeenCalledWith('equipment_search', { q: 'op', kind: 'amp' });
	});

	const rows: [string, () => Promise<unknown>, string, Record<string, unknown>][] = [
		['getEquipmentLibrary', () => api.getEquipmentLibrary(), 'equipment_library', {}],
		[
			'addToLibrary',
			() => api.addToLibrary({ type: 'weapon', catalog_id: 'w1' }),
			'equipment_add',
			{ req: { type: 'weapon', catalog_id: 'w1' } },
		],
		[
			'updateLibrary coerces the string id to a number',
			() => api.updateLibrary('7', { type: 'weapon' }),
			'equipment_update',
			{ item_id: 7, req: { type: 'weapon' } },
		],
		[
			'removeFromLibrary coerces the string id to a number',
			() => api.removeFromLibrary('7'),
			'equipment_delete',
			{ item_id: 7 },
		],
		[
			'getEquipmentDetail coerces the string id to a number',
			() => api.getEquipmentDetail('7'),
			'equipment_detail',
			{ item_id: 7 },
		],
	];
	it.each(rows)('%s', async (_name, call, command, args) => {
		await call();
		expect(tauriInvoke).toHaveBeenCalledTimes(1);
		expect(tauriInvoke).toHaveBeenCalledWith(command, args);
	});

	it('maps a typed error payload onto the thrown ApiError contract', async () => {
		tauriInvoke.mockRejectedValue({ kind: 'notFound', message: 'Equipment item 9 not found' });
		const failure = api.getEquipmentDetail('9');
		await expect(failure).rejects.toBeInstanceOf(FakeApiError);
		await expect(failure).rejects.toMatchObject({
			status: 404,
			message: 'Equipment item 9 not found',
		});
	});

	it('maps a message-less kind onto its fixed message', async () => {
		tauriInvoke.mockRejectedValue({ kind: 'unavailable' });
		await expect(api.getEquipmentLibrary()).rejects.toMatchObject({
			status: 503,
			message: 'backend substrate not ready',
		});
	});

	it('surfaces an out-of-contract rejection verbatim', async () => {
		tauriInvoke.mockRejectedValue('command equipment_detail not found');
		await expect(api.getEquipmentDetail('7')).rejects.toMatchObject({
			status: 500,
			message: 'command equipment_detail not found',
		});
	});
});

describe('settings wrappers dispatch typed commands', () => {
	const rows: [string, () => Promise<unknown>, string, Record<string, unknown>][] = [
		['getSettings', () => api.getSettings(), 'settings_get', {}],
		[
			'updateSettings passes the partial patch through',
			() => api.updateSettings({ player_name: 'Mikel' }),
			'settings_update',
			{ patch: { player_name: 'Mikel' } },
		],
		['getOverlayPosition', () => api.getOverlayPosition(), 'settings_overlay_position', {}],
	];
	it.each(rows)('%s', async (_name, call, command, args) => {
		await call();
		expect(tauriInvoke).toHaveBeenCalledTimes(1);
		expect(tauriInvoke).toHaveBeenCalledWith(command, args);
	});

	it('saveOverlayPosition invokes the typed command with x and y and resolves void', async () => {
		tauriInvoke.mockResolvedValue(undefined);
		await expect(api.saveOverlayPosition(120, 48)).resolves.toBeUndefined();
		expect(tauriInvoke).toHaveBeenCalledWith('settings_set_overlay_position', { x: 120, y: 48 });
	});

	it('maps a typed error payload onto the thrown ApiError contract', async () => {
		tauriInvoke.mockRejectedValue({ kind: 'badRequest', message: 'No fields to update' });
		const failure = api.updateSettings({});
		await expect(failure).rejects.toBeInstanceOf(FakeApiError);
		await expect(failure).rejects.toMatchObject({ status: 400, message: 'No fields to update' });
	});
});

describe('codex wrappers dispatch typed commands', () => {
	const rows: [string, () => Promise<unknown>, string, Record<string, unknown>][] = [
		['getCodexSpecies', () => api.getCodexSpecies(), 'codex_species', {}],
		[
			'getCodexSpeciesRanks',
			() => api.getCodexSpeciesRanks('Atrox'),
			'codex_species_ranks',
			{ species_name: 'Atrox' },
		],
		[
			'claimCodexRank',
			() => api.claimCodexRank('Atrox', 3, 'Laser Sniper'),
			'codex_claim',
			{ species_name: 'Atrox', rank: 3, skill_name: 'Laser Sniper' },
		],
		[
			'unclaimCodexRank',
			() => api.unclaimCodexRank('Atrox'),
			'codex_unclaim',
			{ species_name: 'Atrox' },
		],
		[
			'calibrateCodex',
			() => api.calibrateCodex('Atrox', 3),
			'codex_calibrate',
			{ species_name: 'Atrox', rank: 3 },
		],
		[
			'getCodexRecommendation passes an explicit target and profession',
			() => api.getCodexRecommendation('Atrox', 3, { target: 'hp', profession: 'Sniper (Hit)' }),
			'codex_recommend',
			{ species_name: 'Atrox', rank: 3, profession: 'Sniper (Hit)', target: 'hp' },
		],
		[
			'getCodexRecommendation defaults to the profession target and a null profession',
			() => api.getCodexRecommendation('Atrox', 3),
			'codex_recommend',
			{ species_name: 'Atrox', rank: 3, profession: null, target: 'profession' },
		],
		['getCodexMetaAttributes', () => api.getCodexMetaAttributes(), 'codex_meta_attributes', {}],
		[
			'claimCodexMeta',
			() => api.claimCodexMeta('Strength'),
			'codex_meta_claim',
			{ attribute_name: 'Strength' },
		],
	];
	it.each(rows)('%s', async (_name, call, command, args) => {
		await call();
		expect(tauriInvoke).toHaveBeenCalledTimes(1);
		expect(tauriInvoke).toHaveBeenCalledWith(command, args);
	});

	it('maps the not-found rank lookup onto the thrown ApiError contract', async () => {
		tauriInvoke.mockRejectedValue({ kind: 'notFound', message: "Species 'No Such' not found" });
		const failure = api.getCodexSpeciesRanks('No Such');
		await expect(failure).rejects.toBeInstanceOf(FakeApiError);
		await expect(failure).rejects.toMatchObject({
			status: 404,
			message: "Species 'No Such' not found",
		});
	});
});

describe('quests wrappers dispatch typed commands', () => {
	const rows: [string, () => Promise<unknown>, string, Record<string, unknown>][] = [
		['getQuests', () => api.getQuests(), 'quests_list', {}],
		['getQuest coerces the string id', () => api.getQuest('5'), 'quest_get', { quest_id: 5 }],
		[
			'createQuest',
			() => api.createQuest({ name: 'Iron' } as never),
			'quest_create',
			{ input: { name: 'Iron' } },
		],
		[
			'updateQuest',
			() => api.updateQuest('5', { name: 'Iron II' } as never),
			'quest_update',
			{ quest_id: 5, input: { name: 'Iron II' } },
		],
		['startQuest', () => api.startQuest('5'), 'quest_start', { quest_id: 5 }],
		['completeQuest', () => api.completeQuest('5'), 'quest_complete', { quest_id: 5 }],
		[
			'cancelQuest defaults undo_reward to false',
			() => api.cancelQuest('5'),
			'quest_cancel',
			{ quest_id: 5, undo_reward: false },
		],
		[
			'cancelQuest passes an explicit undo_reward',
			() => api.cancelQuest('5', true),
			'quest_cancel',
			{ quest_id: 5, undo_reward: true },
		],
		['getQuestAnalytics', () => api.getQuestAnalytics(), 'quests_analytics', {}],
		['getPlaylistAnalytics', () => api.getPlaylistAnalytics(), 'playlists_analytics', {}],
		['getPlaylists', () => api.getPlaylists(), 'playlists_list', {}],
		[
			'createPlaylist',
			() => api.createPlaylist({ name: 'dailies' } as never),
			'playlist_create',
			{ input: { name: 'dailies' } },
		],
		[
			'updatePlaylist',
			() => api.updatePlaylist('9', { name: 'weeklies' } as never),
			'playlist_update',
			{ playlist_id: 9, input: { name: 'weeklies' } },
		],
	];
	it.each(rows)('%s', async (_name, call, command, args) => {
		await call();
		expect(tauriInvoke).toHaveBeenCalledTimes(1);
		expect(tauriInvoke).toHaveBeenCalledWith(command, args);
	});

	it('deleteQuest invokes the void typed command and resolves void', async () => {
		await expect(api.deleteQuest('5')).resolves.toBeUndefined();
		expect(tauriInvoke).toHaveBeenCalledWith('quest_delete', { quest_id: 5 });
	});

	it('deletePlaylist invokes the void typed command and resolves void', async () => {
		await expect(api.deletePlaylist('9')).resolves.toBeUndefined();
		expect(tauriInvoke).toHaveBeenCalledWith('playlist_delete', { playlist_id: 9 });
	});
});

describe('suggestion lookups', () => {
	it('getTrackingTagSuggestions short-circuits on blank input and trims the query', async () => {
		await expect(api.getTrackingTagSuggestions('   ')).resolves.toEqual([]);
		expect(clientGet).not.toHaveBeenCalled();

		await api.getTrackingTagSuggestions('  team ');
		expect(clientGet).toHaveBeenCalledWith('/api/tracking/tag-suggestions', {
			params: { query: { q: 'team' } },
		});
	});

	it('getManualMobSuggestions short-circuits on blank input and trims the query', async () => {
		await expect(api.getManualMobSuggestions('')).resolves.toEqual([]);
		expect(clientGet).not.toHaveBeenCalled();

		await api.getManualMobSuggestions(' atrox ');
		expect(clientGet).toHaveBeenCalledWith('/api/tracking/manual-mob-suggestions', {
			params: { query: { q: 'atrox' } },
		});
	});
});

describe('getManualSkillScanPending', () => {
	it('returns the pending payload when present', async () => {
		clientGet.mockResolvedValue({ data: { skills: { Anatomy: 12 } } });
		await expect(api.getManualSkillScanPending()).resolves.toEqual({
			skills: { Anatomy: 12 },
		});
	});

	it('maps a 404 to null (no pending result is an expected state)', async () => {
		clientGet.mockRejectedValue(new FakeApiError(404, 'no pending scan'));
		await expect(api.getManualSkillScanPending()).resolves.toBeNull();
	});

	it('rethrows any other ApiError status', async () => {
		clientGet.mockRejectedValue(new FakeApiError(500, 'broken'));
		await expect(api.getManualSkillScanPending()).rejects.toMatchObject({ status: 500 });
	});

	it('rethrows non-ApiError failures', async () => {
		clientGet.mockRejectedValue(new TypeError('network down'));
		await expect(api.getManualSkillScanPending()).rejects.toBeInstanceOf(TypeError);
	});
});

describe('character wrappers dispatch typed commands', () => {
	const rows: [string, () => Promise<unknown>, string, Record<string, unknown>][] = [
		['getCalibrationStatus', () => api.getCalibrationStatus(), 'character_calibration', {}],
		['getCharacterStats', () => api.getCharacterStats(), 'character_stats', {}],
		['getCharacterSkills', () => api.getCharacterSkills(), 'character_skills', {}],
		['getCharacterProfessions', () => api.getCharacterProfessions(), 'character_professions', {}],
		[
			'getProfessionOptimizer',
			() => api.getProfessionOptimizer('Sniper (Hit)'),
			'character_profession_optimizer',
			{ profession: 'Sniper (Hit)' },
		],
		['getHpOptimizer', () => api.getHpOptimizer(), 'character_hp_optimizer', {}],
		[
			'getCharacterProspectOptions',
			() => api.getCharacterProspectOptions(),
			'character_prospect_options',
			{},
		],
	];
	it.each(rows)('%s', async (_name, call, command, args) => {
		await call();
		expect(tauriInvoke).toHaveBeenCalledTimes(1);
		expect(tauriInvoke).toHaveBeenCalledWith(command, args);
	});

	it('maps a typed error payload onto the thrown ApiError contract', async () => {
		tauriInvoke.mockRejectedValue({ kind: 'internal' });
		await expect(api.getCharacterStats()).rejects.toMatchObject({
			status: 500,
			message: 'Internal Server Error',
		});
	});
});

describe('getProfessionPathOptimizer dispatches the typed command', () => {
	it('maps a targetLevel goal onto the target_level argument, ped_budget null', async () => {
		await api.getProfessionPathOptimizer('Sniper (Hit)', { targetLevel: 40 });
		expect(tauriInvoke).toHaveBeenCalledWith('character_path_optimizer', {
			profession: 'Sniper (Hit)',
			target_level: 40,
			ped_budget: null,
		});
	});

	it('maps a pedBudget goal onto the ped_budget argument, target_level null', async () => {
		await api.getProfessionPathOptimizer('Sniper (Hit)', { pedBudget: 250 });
		expect(tauriInvoke).toHaveBeenCalledWith('character_path_optimizer', {
			profession: 'Sniper (Hit)',
			target_level: null,
			ped_budget: 250,
		});
	});
});

describe('getCharacterProspect dispatches the typed command', () => {
	it('omits sliceValue for the global slice even when one is supplied', async () => {
		await api.getCharacterProspect({
			profession: 'Sniper (Hit)',
			targetLevel: 40,
			sliceType: 'global',
			sliceValue: 'ignored',
		});
		expect(tauriInvoke).toHaveBeenCalledWith('character_prospect', {
			query: { profession: 'Sniper (Hit)', targetLevel: 40, sliceType: 'global' },
		});
	});

	it('omits sliceValue when it is absent on a non-global slice', async () => {
		await api.getCharacterProspect({
			profession: 'Sniper (Hit)',
			targetLevel: 40,
			sliceType: 'mob',
			sliceValue: null,
		});
		expect(tauriInvoke).toHaveBeenCalledWith('character_prospect', {
			query: { profession: 'Sniper (Hit)', targetLevel: 40, sliceType: 'mob' },
		});
	});

	it('passes sliceValue for a non-global slice', async () => {
		await api.getCharacterProspect({
			profession: 'Sniper (Hit)',
			targetLevel: 40,
			sliceType: 'mob',
			sliceValue: 'Atrox',
		});
		expect(tauriInvoke).toHaveBeenCalledWith('character_prospect', {
			query: {
				profession: 'Sniper (Hit)',
				targetLevel: 40,
				sliceType: 'mob',
				sliceValue: 'Atrox',
			},
		});
	});

	it('includes markupUplift only when strictly positive', async () => {
		await api.getCharacterProspect({
			profession: 'Sniper (Hit)',
			targetLevel: 40,
			sliceType: 'global',
			markupUplift: 0,
		});
		expect(tauriInvoke.mock.calls[0][1]).toEqual({
			query: { profession: 'Sniper (Hit)', targetLevel: 40, sliceType: 'global' },
		});

		tauriInvoke.mockClear();
		tauriInvoke.mockResolvedValue(DATA);
		await api.getCharacterProspect({
			profession: 'Sniper (Hit)',
			targetLevel: 40,
			sliceType: 'global',
			markupUplift: 1.05,
		});
		expect(tauriInvoke.mock.calls[0][1]).toEqual({
			query: {
				profession: 'Sniper (Hit)',
				targetLevel: 40,
				sliceType: 'global',
				markupUplift: 1.05,
			},
		});
	});
});

describe('re-exported client surface', () => {
	it('forwards ApiError, request, and the asset helpers from ./client', async () => {
		expect(api.ApiError).toBe(FakeApiError);
		expect(await api.manualSkillScanCapturePng(2)).toBe('data:image/png;base64,page2');
		expect(typeof api.request).toBe('function');
	});
});
