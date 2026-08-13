<script lang="ts">
	import { onMount } from 'svelte';
	import { Button, ErrorNotice, Tabs } from '$lib/components';
	import FamilyFormModal from '$lib/features/quests/FamilyFormModal.svelte';
	import FamilyListView from '$lib/features/quests/FamilyListView.svelte';
	import { createFamilyModel } from '$lib/features/quests/familyModel.svelte';
	import PlaylistFormModal from '$lib/features/quests/PlaylistFormModal.svelte';
	import PlaylistListView from '$lib/features/quests/PlaylistListView.svelte';
	import { createPlaylistModel } from '$lib/features/quests/playlistModel.svelte';
	import QuestAnalyticsView from '$lib/features/quests/QuestAnalyticsView.svelte';
	import QuestFormModal from '$lib/features/quests/QuestFormModal.svelte';
	import QuestListView from '$lib/features/quests/QuestListView.svelte';
	import QuestRewardReview from '$lib/features/quests/QuestRewardReview.svelte';
	import { createQuestsModel } from '$lib/features/quests/questsModel.svelte';
	import { closeGuide, openGuide } from '$lib/guide/engine';
	import { guideState, registerDemoApi, unregisterDemoApi } from '$lib/guide/state.svelte';
	import { questsSurface } from '$lib/guide/surfaces/quests';
	import { getPreference } from '$lib/preferences';
	import { useVisiblePoll } from '$lib/realtime/useVisiblePoll';
	import { hydrate, subscribeTracking, trackingSnapshot } from '$lib/stores/trackingStore.svelte';

	const model = createQuestsModel();
	const playlistModel = createPlaylistModel(model);
	const familyModel = createFamilyModel({
		get families() {
			return model.families;
		},
		set families(value) {
			model.families = value;
		},
		get error() {
			return model.error;
		},
		set error(value) {
			model.error = value;
		},
		get deleteConfirmId() {
			return model.deleteConfirmId;
		},
		set deleteConfirmId(value) {
			model.deleteConfirmId = value;
		},
		refreshQuests: () => model.refresh(),
	});

	// View toggle
	let view: 'quests' | 'families' | 'playlists' | 'review' | 'analytics' = $state('quests');

	// Cooldown tick
	let now = $state(Date.now());

	let trackingActive = $derived(trackingSnapshot.current?.status === 'active');

	// Guide
	let guideSeen = $state(true);
	function toggleSurfaceGuide(): void {
		if (guideState.isActive) {
			closeGuide();
		} else {
			guideSeen = true;
			void openGuide(questsSurface);
		}
	}

	onMount(() => {
		void (async () => {
			guideSeen = await getPreference<boolean>('guide_seen_quests', false);
		})();
		const stopClock = useVisiblePoll(() => { now = Date.now(); }, { intervalMs: 1000 });
		registerDemoApi('quests', {
			setView: (v: string) => {
				view = v as 'quests' | 'families' | 'playlists' | 'review' | 'analytics';
			},
			openNewQuestModal: () => {
				model.openNewQuest();
			},
			closeNewQuestModal: () => {
				model.showQuestModal = false;
				model.editingQuest = null;
			},
			closePlaylistModal: () => {
				playlistModel.showPlaylistModal = false;
				playlistModel.editingPlaylist = null;
			},
			closeFamilyModal: () => {
				familyModel.showFamilyModal = false;
				familyModel.editingFamily = null;
			}
		});
		return () => {
			stopClock();
			unregisterDemoApi('quests');
		};
	});

	// Quest data refreshes every 10s while tracking is active (below) to pick up
	// chat.log mission-completion lines. The active/idle signal that gates it is
	// event-driven: hydrate the tracking snapshot once, then keep it current from
	// pushed session frames rather than polling for session start/stop.
	$effect(() => {
		if (guideState.isActive) return;
		// Subscribe-then-hydrate (the canonical consumer discipline): attach the
		// tracking listener first so a frame landing during the initial read is
		// re-announced rather than lost. The hydrate stays independent of the
		// listen() promise (which never resolves in the e2e shell), so the first
		// read always runs; a frame arriving before the listener attaches is the
		// only residual gap, far smaller than reading before subscribing at all.
		let unsubscribe: (() => void) | undefined;
		let stopped = false;
		void subscribeTracking().then((un) => {
			// Guard the teardown-before-resolve race: detach immediately if the
			// effect already cleaned up rather than leaking the listener.
			if (stopped) un();
			else unsubscribe = un;
		});
		void hydrate();
		return () => {
			stopped = true;
			unsubscribe?.();
		};
	});

	$effect(() => {
		if (guideState.isActive) return;
		if (!trackingActive) return;
		return useVisiblePoll(() => model.refresh(), { intervalMs: 10000, immediate: false });
	});

	// Reload data on initial mount and whenever guide-mode toggles.
	$effect(() => {
		void model.loadData(guideState.isActive);
	});

	// Lazy-load analytics on first entry to the analytics tab.
	$effect(() => {
		if (guideState.isActive) return;
		if (view === 'analytics' && !model.analyticsLoaded && !model.analyticsLoading) {
			model.loadAnalytics();
		}
	});
</script>

<div class="px-6 pb-6 space-y-4">
	<!-- Header -->
	<div class="flex items-center justify-between">
		<header class="flex flex-col gap-1.5">
			<h1 class="text-xl font-semibold text-text tracking-tight">Quests</h1>
			<span class="block h-px w-12 bg-gradient-to-r from-accent/60 to-transparent"></span>
			<p class="text-sm text-text-secondary mt-0.5">Track missions, manage cooldowns, build hunt playlists</p>
		</header>
		<div class="flex items-center gap-2">
			<button
				type="button"
				onclick={toggleSurfaceGuide}
				title={guideState.isActive ? 'Exit guide' : 'Open guide'}
				aria-label={guideState.isActive ? 'Exit guide' : 'Open guide for this page'}
				class="relative h-8 w-8 rounded-full border border-border bg-surface hover:bg-surface-hover text-text-secondary hover:text-text transition-colors flex items-center justify-center text-sm font-semibold {guideState.isActive ? 'z-[9100]' : ''}"
			>
				{#if guideState.isActive}
					<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="currentColor" class="w-3.5 h-3.5" aria-hidden="true">
						<path d="M5.28 4.22a.75.75 0 00-1.06 1.06L6.94 8l-2.72 2.72a.75.75 0 101.06 1.06L8 9.06l2.72 2.72a.75.75 0 101.06-1.06L9.06 8l2.72-2.72a.75.75 0 00-1.06-1.06L8 6.94 5.28 4.22z" />
					</svg>
				{:else}
					?
				{/if}
				{#if !guideSeen}
					<span class="absolute -top-0.5 -right-0.5 h-2 w-2 rounded-full bg-accent"></span>
				{/if}
			</button>
			<Button size="sm" variant="secondary" onclick={() => model.openNewQuest()}>
				{#snippet children()}+ Quest{/snippet}
			</Button>
			<Button size="sm" variant="secondary" onclick={() => familyModel.openNewFamily()}>
				{#snippet children()}+ Family{/snippet}
			</Button>
			<Button size="sm" variant="secondary" onclick={() => playlistModel.openNewPlaylist()}>
				{#snippet children()}+ Playlist{/snippet}
			</Button>
		</div>
	</div>

	<ErrorNotice message={model.error} />

	<!-- Main tab toggle -->
	<Tabs
		tabs={[
			{ id: 'quests', label: 'Quests' },
			{ id: 'families', label: 'Families' },
			{ id: 'playlists', label: 'Playlists' },
			{ id: 'review', label: 'Reward Review' },
			{ id: 'analytics', label: 'Analytics' }
		]}
		active={view}
		onchange={(id) => (view = id as 'quests' | 'families' | 'playlists' | 'review' | 'analytics')}
	/>

	{#if model.loading}
		<div class="text-sm text-text-tertiary py-8 text-center">Loading quests...</div>
	{:else if view === 'quests'}
		<QuestListView {model} {now} />
	{:else if view === 'families'}
		<FamilyListView model={familyModel} questsModel={model} {now} />
	{:else if view === 'playlists'}
		<PlaylistListView model={playlistModel} questsModel={model} {now} />
	{:else if view === 'review'}
		<QuestRewardReview />
	{:else if view === 'analytics'}
		<QuestAnalyticsView {model} />
	{/if}
</div>

<QuestFormModal {model} />
<FamilyFormModal model={familyModel} />
<PlaylistFormModal model={playlistModel} />
