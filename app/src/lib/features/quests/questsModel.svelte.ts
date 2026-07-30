/**
 * Quest-surface view model: the quest/playlist data set, the quest-view
 * filters and grouping, the quest lifecycle and CRUD handlers, the quest
 * form modal state, and the analytics loads. Presentation lives in the
 * feature components; they compose over this state.
 */

import {
	cancelQuest,
	completeQuest,
	createQuest,
	deleteQuest,
	getAnalyticsOverview,
	getPlaylistAnalytics,
	getPlaylists,
	getQuestAnalytics,
	getQuests,
	startQuest,
	updateQuest,
} from '$lib/api';
import {
	questsDemoGlobalLiquidReturnRate,
	questsDemoGlobalSkillProgressionRate,
	questsDemoPlaylistAnalytics,
	questsDemoPlaylists,
	questsDemoQuestAnalytics,
	questsDemoQuests,
} from '$lib/guide/fixtures/quests';
import type {
	PlaylistAnalyticsRow,
	Quest,
	QuestAnalyticsRow,
	QuestCreateData,
	QuestPlaylist,
} from '$lib/types';
import { describeError } from '$lib/view/errorState';
import { getCooldownStatus } from './cooldown';
import { type GlobalRates, globalRates, type RewardMode } from './economics';

/** The planet options the quest and playlist forms offer. */
export const PLANETS = [
	'Calypso',
	'ARIS',
	'Arkadia',
	'Cyrene',
	'Monria',
	'Toulan',
	'Rocktropia',
	'Next Island',
	'Ancient Greece',
] as const;

export type CooldownUnit = 'hours' | 'days';

export interface QuestFormState {
	name: string;
	planet: string;
	category: string;
	waypoint: string;
	cooldown_hours: number | null;
	reward_ped: number | null;
	reward_is_skill: boolean;
	expected_reward_markup_percent: number | null;
	reward_description: string;
	notes: string;
	chain_name: string;
	chain_position: number | null;
	chain_total: number | null;
	mobs: string[];
	/** The signal loot item: set makes this a signal-completed quest
	 * (focusing starts it; the item's arrival in a mission-less loot
	 * pickup completes it). Exclusive with a positive reward. */
	signal_loot_item: string;
}

function defaultQuestForm(): QuestFormState {
	return {
		name: '',
		planet: 'Calypso',
		category: '',
		waypoint: '',
		cooldown_hours: null,
		reward_ped: null,
		reward_is_skill: false,
		expected_reward_markup_percent: null,
		reward_description: '',
		notes: '',
		chain_name: '',
		chain_position: null,
		chain_total: null,
		mobs: [],
		signal_loot_item: '',
	};
}

export function createQuestsModel() {
	// ── Data ──
	let quests = $state<Quest[]>([]);
	let playlists = $state<QuestPlaylist[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	// ── Quest view state ──
	let searchQuery = $state('');
	let selectedPlanet = $state<string | null>(null);
	let selectedMob = $state<string | null>(null);
	let collapsedCategories = $state<Set<string>>(new Set());
	let categoriesInitialised = false;

	// ── Row interactions ──
	let pendingCancelChoiceQuestId = $state<string | null>(null);
	let copiedWp = $state<string | null>(null);
	let deleteConfirmId = $state<string | null>(null);

	// ── Quest modal ──
	let showQuestModal = $state(false);
	let editingQuest = $state<Quest | null>(null);
	let questForm = $state(defaultQuestForm());
	let mobInput = $state('');
	let cooldownUnit = $state<CooldownUnit>('hours');
	let cooldownInput = $state<number | null>(null);

	// ── Analytics ──
	let analyticsData = $state<QuestAnalyticsRow[]>([]);
	let playlistAnalyticsData = $state<PlaylistAnalyticsRow[]>([]);
	let analyticsLoading = $state(false);
	let analyticsError = $state<string | null>(null);
	let analyticsLoaded = $state(false);
	let rates = $state<GlobalRates>({ liquidReturnRate: 0, skillProgressionRate: 0 });
	let analyticsRewardMode = $state<RewardMode>('tt');

	// ── Computed: planets from data ──
	const planets = $derived([...new Set(quests.map((q) => q.planet))].sort());

	// ── Computed: mobs available on current planet filter ──
	const planetQuests = $derived(
		selectedPlanet ? quests.filter((q) => q.planet === selectedPlanet) : quests,
	);
	const mobs = $derived([...new Set(planetQuests.flatMap((q) => q.targetMobs))].sort());

	// ── Computed: quest filtering (planet + mob + search) ──
	const filteredQuests = $derived.by(() => {
		let result = planetQuests;
		if (selectedMob) {
			const mob = selectedMob;
			result = result.filter((q) => q.targetMobs.includes(mob));
		}
		if (searchQuery) {
			const s = searchQuery.toLowerCase();
			result = result.filter(
				(q) =>
					q.name.toLowerCase().includes(s) ||
					q.targetMobs.some((m) => m.toLowerCase().includes(s)) ||
					q.planet.toLowerCase().includes(s) ||
					(q.category?.toLowerCase().includes(s) ?? false),
			);
		}
		return result;
	});

	const questsByCategory = $derived.by(() => {
		const groups: { category: string; quests: Quest[] }[] = [];
		const catMap = new Map<string, Quest[]>();
		const uncategorised: Quest[] = [];

		for (const q of filteredQuests) {
			if (q.category) {
				if (!catMap.has(q.category)) catMap.set(q.category, []);
				catMap.get(q.category)?.push(q);
			} else {
				uncategorised.push(q);
			}
		}

		if (uncategorised.length > 0) {
			groups.push({ category: '', quests: uncategorised });
		}
		for (const [cat, qs] of catMap) {
			groups.push({ category: cat, quests: qs });
		}
		return groups;
	});

	// Start with all categories collapsed (first load only).
	function initialiseCollapsedCategories(source: Quest[]) {
		if (categoriesInitialised) return;
		const cats = new Set<string>();
		for (const quest of source) {
			if (quest.category) cats.add(quest.category);
		}
		collapsedCategories = cats;
		categoriesInitialised = true;
	}

	async function loadData(guideMode: boolean) {
		loading = true;
		error = null;
		try {
			if (guideMode) {
				quests = questsDemoQuests.map((q) => ({ ...q }));
				playlists = questsDemoPlaylists.map((p) => ({ ...p }));
				analyticsData = questsDemoQuestAnalytics.map((a) => ({ ...a }));
				playlistAnalyticsData = questsDemoPlaylistAnalytics.map((a) => ({ ...a }));
				rates = {
					liquidReturnRate: questsDemoGlobalLiquidReturnRate,
					skillProgressionRate: questsDemoGlobalSkillProgressionRate,
				};
				analyticsLoaded = true;
				analyticsError = null;
				initialiseCollapsedCategories(quests);
				return;
			}
			// Leaving guide mode must drop the seeded demo analytics: re-arm
			// the lazy analytics load so the next visit reads live data.
			analyticsLoaded = false;
			const [q, p] = await Promise.all([getQuests(), getPlaylists()]);
			quests = q;
			playlists = p;
			initialiseCollapsedCategories(q);
		} catch (e) {
			error = describeError(e, 'Failed to load quests');
		} finally {
			loading = false;
		}
	}

	async function refresh() {
		try {
			const [q, p] = await Promise.all([getQuests(), getPlaylists()]);
			quests = q;
			playlists = p;
			// A refresh can drop a quest (deleted elsewhere); a pending cancel
			// choice on a vanished quest would otherwise dangle forever.
			if (
				pendingCancelChoiceQuestId &&
				!q.some((quest) => quest.id === pendingCancelChoiceQuestId)
			) {
				pendingCancelChoiceQuestId = null;
			}
		} catch {
			// Deliberate swallow: this is the background tracking-active poll; a
			// failed tick keeps the last good data and the next tick retries.
		}
	}

	async function loadAnalytics() {
		analyticsLoading = true;
		analyticsError = null;
		try {
			const [qAnalytics, plAnalytics, overview] = await Promise.all([
				getQuestAnalytics(),
				getPlaylistAnalytics(),
				getAnalyticsOverview('all'),
			]);
			analyticsData = qAnalytics;
			playlistAnalyticsData = plAnalytics;
			rates = globalRates(overview);
			analyticsLoaded = true;
		} catch (e) {
			analyticsError = describeError(e, 'Failed to load quest analytics');
		} finally {
			analyticsLoading = false;
		}
	}

	// ── Category status summary ──
	function categoryStatusCounts(
		qs: Quest[],
		now: number,
	): { ready: number; started: number; cooling: number } {
		let ready = 0;
		let started = 0;
		let cooling = 0;
		for (const q of qs) {
			if (q.startedAt) started++;
			else if (getCooldownStatus(q, now) === 'cooling') cooling++;
			else ready++;
		}
		return { ready, started, cooling };
	}

	// ── Quest actions ──
	async function handleStart(questId: string) {
		error = null;
		try {
			const updated = await startQuest(questId);
			quests = quests.map((q) => (q.id === updated.id ? updated : q));
			if (pendingCancelChoiceQuestId === questId) pendingCancelChoiceQuestId = null;
		} catch (e) {
			error = describeError(e, 'Failed to start quest');
		}
	}

	async function handleComplete(questId: string) {
		error = null;
		try {
			const updated = await completeQuest(questId);
			quests = quests.map((q) => (q.id === updated.id ? updated : q));
			if (pendingCancelChoiceQuestId === questId) pendingCancelChoiceQuestId = null;
		} catch (e) {
			error = describeError(e, 'Failed to complete quest');
		}
	}

	async function handleCancel(questId: string, undoReward = false) {
		error = null;
		try {
			const updated = await cancelQuest(questId, undoReward);
			quests = quests.map((q) => (q.id === updated.id ? updated : q));
			if (pendingCancelChoiceQuestId === questId) pendingCancelChoiceQuestId = null;
		} catch (e) {
			error = describeError(e, 'Failed to cancel quest');
		}
	}

	function toggleCancelChoice(questId: string) {
		pendingCancelChoiceQuestId = pendingCancelChoiceQuestId === questId ? null : questId;
	}

	function copyWaypoint(questId: string, waypoint: string) {
		navigator.clipboard.writeText(waypoint);
		copiedWp = questId;
		setTimeout(() => {
			if (copiedWp === questId) copiedWp = null;
		}, 1500);
	}

	// ── Quest CRUD ──
	function openNewQuest() {
		editingQuest = null;
		questForm = defaultQuestForm();
		cooldownUnit = 'hours';
		cooldownInput = null;
		mobInput = '';
		showQuestModal = true;
	}

	function openEditQuest(quest: Quest) {
		editingQuest = quest;
		const h = quest.cooldownDurationHours;
		if (h != null && h >= 24 && h % 24 === 0) {
			cooldownUnit = 'days';
			cooldownInput = h / 24;
		} else {
			cooldownUnit = 'hours';
			cooldownInput = h;
		}
		questForm = {
			name: quest.name,
			planet: quest.planet,
			category: quest.category ?? '',
			waypoint: quest.waypoint ?? '',
			cooldown_hours: h,
			reward_ped: quest.reward,
			reward_is_skill: quest.rewardIsSkill,
			expected_reward_markup_percent: quest.expectedRewardMarkupPercent,
			reward_description: quest.rewardDescription,
			notes: quest.notes,
			chain_name: quest.chainName ?? '',
			chain_position: quest.chainPosition,
			chain_total: quest.chainTotal,
			mobs: [...quest.targetMobs],
			signal_loot_item: quest.signalLootItem ?? '',
		};
		mobInput = '';
		showQuestModal = true;
	}

	async function saveQuest() {
		const cdHours =
			cooldownInput != null ? (cooldownUnit === 'days' ? cooldownInput * 24 : cooldownInput) : null;
		const data: QuestCreateData = {
			name: questForm.name,
			planet: questForm.planet,
			category: questForm.category || null,
			waypoint: questForm.waypoint || null,
			cooldown_hours: cdHours,
			reward_ped: questForm.reward_ped,
			reward_is_skill: questForm.reward_is_skill,
			expected_reward_markup_percent:
				!questForm.reward_is_skill && (questForm.reward_ped ?? 0) > 0
					? questForm.expected_reward_markup_percent
					: null,
			reward_description: questForm.reward_description || null,
			notes: questForm.notes || null,
			chain_name: questForm.chain_name || null,
			chain_position: questForm.chain_position,
			chain_total: questForm.chain_total,
			mobs: questForm.mobs,
			signal_loot_item: questForm.signal_loot_item.trim() || null,
		};
		try {
			if (editingQuest) {
				const updated = await updateQuest(editingQuest.id, data);
				quests = quests.map((q) => (q.id === updated.id ? updated : q));
			} else {
				const created = await createQuest(data);
				quests = [...quests, created];
			}
			showQuestModal = false;
		} catch (e) {
			error = describeError(e, 'Failed to save quest');
		}
	}

	async function handleDeleteQuest(questId: string) {
		error = null;
		try {
			await deleteQuest(questId);
			quests = quests.filter((q) => q.id !== questId);
			deleteConfirmId = null;
		} catch (e) {
			error = describeError(e, 'Failed to delete quest');
		}
	}

	function addMob() {
		const mob = mobInput.trim();
		if (mob && !questForm.mobs.includes(mob)) {
			questForm.mobs = [...questForm.mobs, mob];
		}
		mobInput = '';
	}

	function removeMob(mob: string) {
		questForm.mobs = questForm.mobs.filter((m) => m !== mob);
	}

	function rewardMarkupInputDisabled() {
		return (questForm.reward_ped ?? 0) <= 0;
	}

	return {
		// ── Data ──
		get quests() {
			return quests;
		},
		get playlists() {
			return playlists;
		},
		set playlists(value: QuestPlaylist[]) {
			playlists = value;
		},
		get loading() {
			return loading;
		},
		get error() {
			return error;
		},
		set error(value: string | null) {
			error = value;
		},

		// ── Quest view state ──
		get searchQuery() {
			return searchQuery;
		},
		set searchQuery(value: string) {
			searchQuery = value;
		},
		get selectedPlanet() {
			return selectedPlanet;
		},
		set selectedPlanet(value: string | null) {
			selectedPlanet = value;
		},
		get selectedMob() {
			return selectedMob;
		},
		set selectedMob(value: string | null) {
			selectedMob = value;
		},
		get collapsedCategories() {
			return collapsedCategories;
		},
		set collapsedCategories(value: Set<string>) {
			collapsedCategories = value;
		},

		// ── Row interactions ──
		get pendingCancelChoiceQuestId() {
			return pendingCancelChoiceQuestId;
		},
		get copiedWp() {
			return copiedWp;
		},
		get deleteConfirmId() {
			return deleteConfirmId;
		},
		set deleteConfirmId(value: string | null) {
			deleteConfirmId = value;
		},

		// ── Quest modal ──
		get showQuestModal() {
			return showQuestModal;
		},
		set showQuestModal(value: boolean) {
			showQuestModal = value;
		},
		get editingQuest() {
			return editingQuest;
		},
		set editingQuest(value: Quest | null) {
			editingQuest = value;
		},
		get questForm() {
			return questForm;
		},
		get mobInput() {
			return mobInput;
		},
		set mobInput(value: string) {
			mobInput = value;
		},
		get cooldownUnit() {
			return cooldownUnit;
		},
		set cooldownUnit(value: CooldownUnit) {
			cooldownUnit = value;
		},
		get cooldownInput() {
			return cooldownInput;
		},
		set cooldownInput(value: number | null) {
			cooldownInput = value;
		},

		// ── Analytics ──
		get analyticsData() {
			return analyticsData;
		},
		get playlistAnalyticsData() {
			return playlistAnalyticsData;
		},
		get analyticsLoading() {
			return analyticsLoading;
		},
		get analyticsError() {
			return analyticsError;
		},
		get analyticsLoaded() {
			return analyticsLoaded;
		},
		get rates() {
			return rates;
		},
		get analyticsRewardMode() {
			return analyticsRewardMode;
		},
		set analyticsRewardMode(value: RewardMode) {
			analyticsRewardMode = value;
		},

		// ── Computed ──
		get planets() {
			return planets;
		},
		get mobs() {
			return mobs;
		},
		get filteredQuests() {
			return filteredQuests;
		},
		get questsByCategory() {
			return questsByCategory;
		},

		loadData,
		refresh,
		loadAnalytics,
		categoryStatusCounts,
		handleStart,
		handleComplete,
		handleCancel,
		toggleCancelChoice,
		copyWaypoint,
		openNewQuest,
		openEditQuest,
		saveQuest,
		handleDeleteQuest,
		addMob,
		removeMob,
		rewardMarkupInputDisabled,
	};
}

export type QuestsModel = ReturnType<typeof createQuestsModel>;
