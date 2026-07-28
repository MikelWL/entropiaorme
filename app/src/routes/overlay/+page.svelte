<script lang="ts">
	import {
		ApiError,
		getTrackingSnapshot,
		startTracking,
		stopTracking,
		releaseMob,
		getOverlayPosition,
		saveOverlayPosition,
		getSessionNameSuggestions,
		setSessionConfig,
		getManualMobSuggestions,
		lockManualMob,
		getSessionQuestLinkSuggestion,
		decideSessionQuestLink,
		declareSessionQuest,
		getQuests,
		getPlaylists,
		updateSettings,
		type TrackingLive,
		type TrackingStatus,
		type TrackingSnapshot,
		type ManualMobSuggestion
	} from '$lib/api';
	import { tick, untrack } from 'svelte';
	import { useVisiblePoll, windowGeometryPoll } from '$lib/realtime/useVisiblePoll';
	import { createSnapshotStore } from '$lib/realtime/snapshotStore.svelte';
	import { createPostSessionFlow } from '$lib/features/tracking/postSession.svelte';
	import { createSessionFacets } from '$lib/features/tracking/sessionFacets.svelte';
	import { createTypeahead } from '$lib/view/typeahead.svelte';
	import { getCurrentWindow } from '@tauri-apps/api/window';
	import { PhysicalPosition } from '@tauri-apps/api/dpi';
	import { listen } from '@tauri-apps/api/event';
	import { anchorBelow, anchorCentreBelow, createAnchorTracker } from '$lib/windows/anchor';
	import { createSatelliteWindow } from '$lib/windows/satellite';
	import { createWindowSizeSync } from '$lib/windows/windowSize';
	import {
		OVERLAY_MENU_CLOSED_EVENT,
		OVERLAY_MENU_HIDE_EVENT,
		OVERLAY_MENU_INTERACT_EVENT,
		OVERLAY_MENU_READY_EVENT,
		OVERLAY_MENU_SELECT_EVENT,
		OVERLAY_MENU_SHOW_EVENT,
		OVERLAY_MENU_WINDOW_LABEL,
		type OverlayMenuKind,
		type OverlayMenuSelection,
		type OverlayMenuState
	} from '$lib/windows/overlayMenu';
	import {
		OVERLAY_ARMOUR_COST_CLOSED_EVENT,
		OVERLAY_ARMOUR_COST_HIDE_EVENT,
		OVERLAY_ARMOUR_COST_READY_EVENT,
		OVERLAY_ARMOUR_COST_SHOW_EVENT,
		OVERLAY_ARMOUR_COST_UPDATE_EVENT,
		OVERLAY_ARMOUR_COST_WINDOW_LABEL,
		type OverlayArmourCostState
	} from '$lib/windows/overlayArmourCost';
	import OverlayStrip from '$lib/components/overlay/OverlayStrip.svelte';

	// The colon-form Tauri topic the event relay re-emits each backend tracking
	// frame on (the wire topic `tracking.session.updated`; Tauri event names
	// forbid dots). See lib/realtime/eventRelay.ts.
	const TRACKING_TOPIC = 'tracking:session:updated';
	// Emitted by the shell (toggle_overlay) when this hidden window is shown, so
	// the overlay can re-read config/runtime fields no tracking frame announces.
	const OVERLAY_SHOWN_EVENT = 'overlay-shown';
	const OVERLAY_MENU_VERTICAL_GAP = 6;
	const OVERLAY_MENU_MAX_HEIGHT = 220;
	const OVERLAY_MENU_MAX_WIDTH = 340;
	const OVERLAY_MENU_MIN_WIDTH = 180;

	let overlayRoot: HTMLDivElement | null = $state(null);
	let overlayMenuKind = $state<OverlayMenuKind | null>(null);
	let armourCostOpen = $state(false);
	// Stamped when the popup self-closes (blur, ESC, post-save). The Cost-button
	// click handler races against the CLOSED event: if blur arrives first,
	// armourCostOpen flips to false before toggleArmourCost reads it, and the
	// click would reopen the popup that the same gesture just dismissed.
	// Gating the open branch on this timestamp suppresses that reopen.
	let armourCostClosedAt = 0;
	let armourCostError = $state<string | null>(null);
	let armourCostAnchor: HTMLElement | null = $state(null);
	let postSessionArmourButton: HTMLButtonElement | null = $state(null);
	// Yellow attribution-not-ready warning that replaces the TRACK button when
	// startTracking is refused by the backend (no hotbar slot bound in hotbar
	// mode, or trifecta not configured in trifecta mode). Persists until the
	// user closes it; clicking TRACK again clears it implicitly on success.
	let attributionWarning = $state<string | null>(null);
	let mobInput: HTMLInputElement | null = $state(null);
	let mobInputFocused = $state(false);
	let trifectaSaving = $state(false);
	let trifectaError = $state<string | null>(null);
	let overlayMenuLaunchError = $state<string | null>(null);

	let data = $state<TrackingLive>({ status: 'idle' });
	let status = $state<TrackingStatus | null>(null);
	// Session start in epoch-ms (parsed from the snapshot's started_at), the basis
	// for the client-side elapsed tick. null when no active session is timed.
	let sessionStartedAtMs = $state<number | null>(null);
	let releasing = $state(false);
	let starting = $state(false);

	let mobQuery = $state('');
	// The mob lookup's error channel, shared between the typeahead (search
	// failures, mirrored in by the presenter effect below) and the declare
	// action.
	let mobError = $state<string | null>(null);
	let selectingMob = $state(false);
	let mobCloseTimer: ReturnType<typeof setTimeout> | undefined;


	// The two satellite popovers this window drives. The failure messages keep
	// the overlay's established wording (they render in the strip).
	const menuWindow = createSatelliteWindow({
		label: OVERLAY_MENU_WINDOW_LABEL,
		url: '/overlay-menu',
		width: OVERLAY_MENU_MIN_WIDTH,
		height: 44,
		readyEvent: OVERLAY_MENU_READY_EVENT,
		showEvent: OVERLAY_MENU_SHOW_EVENT,
		hideEvent: OVERLAY_MENU_HIDE_EVENT,
		messages: {
			creationTimeout: 'Popup window creation timed out',
			creationFailed: 'Unknown Tauri popup creation error',
			readyTimeout: 'Popup route did not become ready'
		}
	});
	const armourCostWindow = createSatelliteWindow({
		label: OVERLAY_ARMOUR_COST_WINDOW_LABEL,
		url: '/overlay-armour-cost',
		width: 320,
		height: 64,
		readyEvent: OVERLAY_ARMOUR_COST_READY_EVENT,
		showEvent: OVERLAY_ARMOUR_COST_SHOW_EVENT,
		hideEvent: OVERLAY_ARMOUR_COST_HIDE_EVENT,
		messages: {
			creationTimeout: 'Armour cost popup creation timed out',
			creationFailed: 'Unknown Tauri popup creation error',
			readyTimeout: 'Armour cost popup did not become ready'
		}
	});

	// The consolidated snapshot, event-driven with coalesced re-reads (see the
	// factory). Each webview is its own JS context, so the overlay keeps its
	// own store instance beside the dashboard's.
	const snapshot = createSnapshotStore<TrackingSnapshot>(TRACKING_TOPIC, getTrackingSnapshot);

	// Post-session flow: the armour prompt gating the stop, the final-stats
	// readout, and the quest-link suggestion (see the module for the state
	// machine). Render state comes off `flow`; the deps close over this
	// window's snapshot and armour-cost popup.
	const flow = createPostSessionFlow({
		isSessionActive: () => data.status === 'active',
		isBusy: () => toggling,
		armourReminderEnabled: () => data.endOfSessionArmourReminderEnabled === true,
		refresh: () => snapshot.hydrate(),
		readStats: () => ({
			cost: snapshot.current?.cost ?? 0,
			returns: snapshot.current?.returns ?? 0,
			pes: snapshot.current?.pes ?? 0,
			net: snapshot.current?.net ?? 0
		}),
		stopTracking,
		fetchQuestLinkSuggestion: getSessionQuestLinkSuggestion,
		decideQuestLink: decideSessionQuestLink,
		isArmourPopupOpen: () => armourCostOpen,
		showArmourPopup: showPostSessionArmourPopup,
		onPromptShown: () => {
			void tick().then(scheduleArmourCostAnchorSync);
		}
	});
	const toggling = $derived(starting || flow.stopping);

	// The session facets (name, boost, quest): state, lookups, and writes
	// live in the feature model; this route owns only the popup plumbing
	// the model calls back into.
	const facets = createSessionFacets({
		readFacets: () => ({ name: data.sessionName ?? null, boost: data.skillBoostPercent ?? null }),
		isSessionActive: () => data.status === 'active',
		refresh: () => snapshot.hydrate(),
		searchNames: getSessionNameSuggestions,
		setSessionConfig,
		declareQuest: declareSessionQuest,
		listQuests: getQuests,
		listPlaylists: getPlaylists,
		openNameMenu: () => openNameMenu(),
		closeNameMenu: () => closeNameMenu()
	});

	async function handleDrag(e: MouseEvent) {
		const target = e.target as HTMLElement;
		if (target.closest('button, [role="button"], input, select, textarea')) return;
		if (overlayMenuKind) {
			await hideOverlayMenu();
		}
		if (armourCostOpen) {
			await hideArmourCost();
		}
		await getCurrentWindow().startDragging();
	}

	function clearMobCloseTimer() {
		if (!mobCloseTimer) return;
		clearTimeout(mobCloseTimer);
		mobCloseTimer = undefined;
	}


	function describeOverlayMenuError(error: unknown) {
		if (error instanceof ApiError || error instanceof Error) return error.message;
		if (typeof error === 'string' && error.trim()) return error;
		return 'Popup window failed to open';
	}

	function reportOverlayMenuOpenError(kind: OverlayMenuKind, error: unknown) {
		const message = describeOverlayMenuError(error);
		console.error(`Overlay ${kind} popup failed`, error);
		if (kind === 'trifecta') {
			trifectaError = message;
			return;
		}
		overlayMenuLaunchError = message;
	}

	// Keep this window's OS size in step with the strip; each sync re-anchors
	// the armour-cost popup, which hangs off a strip button.
	const windowSizeSync = createWindowSizeSync(() => overlayRoot, {
		afterSync: () => scheduleArmourCostAnchorSync()
	});

	function measureMenuTextWidth(labels: string[], font = '500 12px Inter, system-ui, sans-serif') {
		if (labels.length === 0) return 0;
		const canvas = document.createElement('canvas');
		const context = canvas.getContext('2d');
		if (!context) return labels.reduce((longest, label) => Math.max(longest, label.length * 8), 0);

		context.font = font;
		return labels.reduce((longest, label) => Math.max(longest, context.measureText(label).width), 0);
	}

	function computeMenuWidth(minWidth: number, labels: string[], padding: number) {
		const contentWidth = measureMenuTextWidth(labels);
		return Math.max(
			Math.ceil(minWidth),
			Math.min(OVERLAY_MENU_MAX_WIDTH, Math.max(OVERLAY_MENU_MIN_WIDTH, Math.ceil(contentWidth + padding)))
		);
	}

	function computeMenuHeight(rows: number) {
		return Math.min(OVERLAY_MENU_MAX_HEIGHT, Math.max(44, rows * 34 + 12));
	}

	function buildMobMenuState(anchorWidth: number): OverlayMenuState | null {
		const trimmedQuery = mobQuery.trim();
		const shouldShow = mobLoading || !!mobError || mobSuggestions.length > 0 || !!trimmedQuery;
		if (!shouldShow) return null;

		const labels = mobLoading
			? ['Searching...']
			: mobError
				? [mobError]
				: (mobSuggestions.length > 0 ? mobSuggestions.map((option) => option.display) : ['No matches']);

		return {
			kind: 'mob',
			width: computeMenuWidth(anchorWidth, labels, 28),
			query: trimmedQuery,
			loading: mobLoading,
			error: mobError,
			mobSuggestions
		};
	}

	function buildNameMenuState(anchorWidth: number): OverlayMenuState | null {
		const trimmedQuery = facets.nameQuery.trim();
		const shouldShow =
			facets.nameLoading || !!facets.nameError || facets.nameSuggestions.length > 0 || !!trimmedQuery;
		if (!shouldShow) return null;

		const labels = facets.nameLoading
			? ['Searching...']
			: facets.nameError
				? [facets.nameError]
				: (facets.nameSuggestions.length > 0
					? facets.nameSuggestions
					: [`Press Enter to name it "${trimmedQuery}"`]);

		return {
			kind: 'name',
			width: computeMenuWidth(anchorWidth, labels, 28),
			query: trimmedQuery,
			loading: facets.nameLoading,
			error: facets.nameError,
			suggestions: facets.nameSuggestions
		};
	}

	function buildQuestMenuState(anchorWidth: number): OverlayMenuState | null {
		const labels = facets.questOptions.length > 0
			? facets.questOptions.map((option) => option.name)
			: ['No active quests'];
		return {
			kind: 'quest',
			width: computeMenuWidth(anchorWidth, labels, 88),
			loading: false,
			error: null,
			options: facets.questOptions
		};
	}

	function buildTrifectaMenuState(anchorWidth: number): OverlayMenuState | null {
		const trifecta = data.trifectaAttribution;
		if (!trifecta || trifecta.presets.length === 0) return null;

		return {
			kind: 'trifecta',
			width: computeMenuWidth(anchorWidth, trifecta.presets.map((preset) => preset.name), 88),
			options: trifecta.presets.map((preset) => ({
				id: preset.id,
				name: preset.name,
				active: preset.id === trifecta.activePresetId
			}))
		};
	}

	async function buildArmourCostState(anchor: HTMLElement): Promise<OverlayArmourCostState | null> {
		const sessionId = armourSessionId;
		if (!sessionId || !anchor.isConnected) return null;

		return {
			sessionId,
			repairOcrEnabled: data.repairOcrEnabled === true,
			anchor: await anchorCentreBelow(anchor, OVERLAY_MENU_VERTICAL_GAP)
		};
	}

	/** Rows the popup will render, which sets its window height. Every
	 * kind falls back to one row: loading, error, and empty states each
	 * occupy exactly one line (the popup route agrees). */
	function menuRowCount(state: OverlayMenuState): number {
		if (state.kind === 'trifecta') return Math.max(1, state.options.length);
		if (state.loading || state.error) return 1;
		if (state.kind === 'name') return Math.max(1, state.suggestions.length);
		if (state.kind === 'mob') return Math.max(1, state.mobSuggestions.length);
		return Math.max(1, state.options.length);
	}

	async function showOverlayMenu(
		kind: OverlayMenuKind,
		anchor: HTMLElement,
		state: OverlayMenuState,
		options: { focusPopup?: boolean } = {}
	) {
		try {
			// Resolve the window (creating it on first use) while the anchor
			// maths runs; the show below re-adopts the settled window.
			const [, anchorPosition] = await Promise.all([
				menuWindow.ensure(),
				anchorBelow(anchor, OVERLAY_MENU_VERTICAL_GAP)
			]);
			const height = computeMenuHeight(menuRowCount(state));

			await menuWindow.show(
				state,
				{ x: anchorPosition.x, y: anchorPosition.y, width: state.width, height },
				{ focus: options.focusPopup }
			);
			if (kind === 'mob') {
				overlayMenuLaunchError = null;
			}
			overlayMenuKind = kind;
		} catch (error) {
			overlayMenuKind = null;
			reportOverlayMenuOpenError(kind, error);
		}
	}

	async function hideOverlayMenu() {
		if (overlayMenuKind === 'mob') {
			clearMobCloseTimer();
		}
		if (overlayMenuKind === 'name') {
			facets.clearNameCloseTimer();
		}
		overlayMenuKind = null;
		await menuWindow.hide();
	}

	async function openMobMenu() {
		if (!mobInput) return;
		const state = buildMobMenuState(mobInput.getBoundingClientRect().width);
		if (!state) return;
		overlayMenuLaunchError = null;
		await showOverlayMenu('mob', mobInput, state);
	}

	async function closeMobMenu() {
		clearMobCloseTimer();
		if (overlayMenuKind !== 'mob') return;
		await hideOverlayMenu();
	}

	async function openNameMenu() {
		if (!facets.nameInput) return;
		const state = buildNameMenuState(facets.nameInput.getBoundingClientRect().width);
		if (!state) return;
		overlayMenuLaunchError = null;
		await showOverlayMenu('name', facets.nameInput, state);
	}

	async function closeNameMenu() {
		facets.clearNameCloseTimer();
		if (overlayMenuKind !== 'name') return;
		await hideOverlayMenu();
	}

	async function toggleQuestMenu(anchor: HTMLButtonElement) {
		if (overlayMenuKind === 'quest') {
			await hideOverlayMenu();
			return;
		}
		if (!(await facets.loadQuestOptions())) return;
		const state = buildQuestMenuState(anchor.getBoundingClientRect().width);
		if (!state) return;
		await showOverlayMenu('quest', anchor, state, { focusPopup: true });
	}

	async function toggleTrifectaMenu(anchor: HTMLButtonElement) {
		if (overlayMenuKind === 'trifecta') {
			await hideOverlayMenu();
			return;
		}

		trifectaError = null;
		const state = buildTrifectaMenuState(anchor.getBoundingClientRect().width);
		if (!state) return;
		await showOverlayMenu('trifecta', anchor, state, { focusPopup: true });
	}

	async function showArmourCost(anchor: HTMLElement) {
		try {
			await armourCostWindow.ensure();
			const state = await buildArmourCostState(anchor);
			if (!state) return;

			armourCostAnchor = anchor;
			// The popup measures its panel, sizes+positions itself accurately, then
			// reveals + focuses on its own (never revealed from here) so it cannot
			// flash for one frame at the wrong (initial-guess) location.
			await armourCostWindow.show(state, undefined, { reveal: false });
			armourCostError = null;
			armourCostOpen = true;
			scheduleArmourCostAnchorSync();
		} catch (error) {
			armourCostOpen = false;
			armourCostAnchor = null;
			armourCostError = error instanceof ApiError || error instanceof Error
				? error.message
				: 'Popup window failed to open';
			console.error('Armour cost popup failed', error);
		}
	}

	async function syncArmourCostAnchor() {
		if (!armourCostOpen || !armourCostAnchor) return;
		const state = await buildArmourCostState(armourCostAnchor);
		if (!state) return;

		await armourCostWindow.emitTo(OVERLAY_ARMOUR_COST_UPDATE_EVENT, state);
	}

	const armourAnchorTracker = createAnchorTracker(() => void syncArmourCostAnchor());

	function scheduleArmourCostAnchorSync() {
		if (!armourCostOpen || !armourCostAnchor) return;
		armourAnchorTracker.schedule();
	}

	function clearArmourCostOpenState() {
		armourCostOpen = false;
		armourCostAnchor = null;
		armourAnchorTracker.cancel();
		flow.notifyArmourPopupClosed();
	}

	async function hideArmourCost() {
		clearArmourCostOpenState();
		await armourCostWindow.hide();
	}

	async function toggleArmourCost(event: MouseEvent) {
		if (armourCostOpen) {
			await hideArmourCost();
			return;
		}
		if (Date.now() - armourCostClosedAt < 250) return;
		const anchor = event.currentTarget as HTMLElement | null;
		if (!anchor) return;
		await showArmourCost(anchor);
	}

	// The armour-cost popup after a Yes on the armour prompt: the anchor
	// button only renders once the post-session readout has, hence the tick.
	async function showPostSessionArmourPopup() {
		await tick();
		if (postSessionArmourButton && armourSessionId && !armourCostOpen) {
			await showArmourCost(postSessionArmourButton);
		}
	}

	async function handleTrifectaPresetSelection(presetId: string) {
		const trifecta = data.trifectaAttribution;
		if (!trifecta || trifectaSaving || presetId === trifecta.activePresetId) return;

		trifectaSaving = true;
		trifectaError = null;
		try {
			await updateSettings({ active_trifecta_preset_id: presetId });
			await snapshot.hydrate();
		} catch (error) {
			trifectaError = error instanceof ApiError || error instanceof Error
				? error.message
				: 'Failed to switch trifecta preset';
		}
		trifectaSaving = false;
	}

	// Restore saved overlay position; periodically persist if moved
	$effect(() => {
		let lastSavedX: number | null = null;
		let lastSavedY: number | null = null;
		let stopPersist: (() => void) | undefined;

		(async () => {
			const win = getCurrentWindow();

			// Restore saved position on mount
			try {
				const pos = await getOverlayPosition();
				if (pos.x != null && pos.y != null) {
					await win.setPosition(new PhysicalPosition(pos.x, pos.y));
					lastSavedX = pos.x;
					lastSavedY = pos.y;
				}
			} catch { /* first launch or backend unreachable */ }

			// Persist position every 5s: save only if changed (avoids onMoved IPC
			// drag interference). windowGeometryPoll keeps running while the overlay
			// is hidden: its hidden/shown state is not reliably observable from
			// inside its own webview, so this is the one poll the visibility gate
			// deliberately does not pause.
			stopPersist = windowGeometryPoll(async () => {
				try {
					const pos = await win.outerPosition();
					if (pos.x !== lastSavedX || pos.y !== lastSavedY) {
						lastSavedX = pos.x;
						lastSavedY = pos.y;
						saveOverlayPosition(pos.x, pos.y).catch(() => {});
					}
				} catch { /* window may be hidden */ }
			}, 5000);
		})();

		return () => {
			stopPersist?.();
		};
	});

	$effect(() => {
		if (!overlayRoot) return;

		windowSizeSync.schedule();

		const handleVisibilityChange = () => {
			if (document.visibilityState === 'visible') {
				windowSizeSync.schedule();
			} else {
				void hideOverlayMenu();
				void hideArmourCost();
			}
		};
		const handleFocus = () => {
			windowSizeSync.schedule();
			scheduleArmourCostAnchorSync();
		};

		const resizeObserver = new ResizeObserver(() => {
			windowSizeSync.schedule();
			scheduleArmourCostAnchorSync();
		});
		resizeObserver.observe(overlayRoot);

		document.addEventListener('visibilitychange', handleVisibilityChange);
		window.addEventListener('focus', handleFocus);

		return () => {
			windowSizeSync.cancel();
			document.removeEventListener('visibilitychange', handleVisibilityChange);
			window.removeEventListener('focus', handleFocus);
			resizeObserver.disconnect();
		};
	});



	// Re-read the consolidated snapshot on each backend tracking frame. The
	// listener attaches FIRST and the initial hydrate runs after it settles,
	// so a frame arriving during subscription setup is not lost (it simply
	// re-triggers a read). A payload-less reconnect nudge on this topic
	// re-hydrates the same way, so it can never be mistaken for an idle
	// session.
	$effect(() => {
		let disposed = false;
		let unlisten: (() => void) | undefined;

		void snapshot.subscribe().then((fn) => {
			if (disposed) {
				fn();
				return;
			}
			unlisten = fn;
			void snapshot.hydrate();
		});

		return () => {
			disposed = true;
			unlisten?.();
		};
	});

	// The overlay is a hidden pre-spawned window shown (not focused) by
	// toggle_overlay, so no focus/visibility event fires on the frontend when it
	// appears. The shell emits `overlay-shown` from the show path; re-read on it
	// to refresh config/runtime fields no tracking frame announces (weapon
	// attribution, trifecta presets, mob-entry mode, repair-OCR, the armour
	// reminder), which would otherwise stay stale and wedge a control after a
	// settings change made while the overlay was hidden.
	$effect(() => {
		let disposed = false;
		let unlisten: (() => void) | undefined;

		(async () => {
			unlisten = await listen(OVERLAY_SHOWN_EVENT, () => {
				if (disposed) return;
				void snapshot.hydrate();
			});
		})();

		return () => {
			disposed = true;
			unlisten?.();
		};
	});

	// Drive the elapsed timer client-side while active. With no poll, data.elapsed
	// would otherwise advance only on coalesced backend frames and the headline
	// timer would stutter; derive seconds from the session's started_at (the same
	// basis the stat pills use) and tick once a second. Idle tears the tick down.
	// Writing data.elapsed here cannot retrigger this effect: Svelte 5 tracks per
	// property, so the .elapsed write is isolated from the .status read (and
	// applySnapshot always materialises the elapsed key, so the write is a
	// value-update, never a key-add that would invalidate the read).
	$effect(() => {
		if (data.status !== 'active' || sessionStartedAtMs == null) return;
		const startedAt = sessionStartedAtMs;
		const tickElapsed = () => {
			data.elapsed = Math.max(0, Math.floor((Date.now() - startedAt) / 1000));
		};
		tickElapsed();
		return useVisiblePoll(tickElapsed, { intervalMs: 1000, immediate: false });
	});

	$effect(() => {
		let disposed = false;
		let unlistenSelect: (() => void) | undefined;
		let unlistenClosed: (() => void) | undefined;
		let unlistenInteract: (() => void) | undefined;

		void (async () => {
			unlistenSelect = await listen<OverlayMenuSelection>(OVERLAY_MENU_SELECT_EVENT, async (event) => {
				if (disposed) return;

				if (event.payload.kind === 'trifecta') {
					overlayMenuKind = null;
					await handleTrifectaPresetSelection(event.payload.presetId);
					return;
				}

				if (event.payload.kind === 'name') {
					overlayMenuKind = null;
					await facets.applyName(event.payload.name);
					return;
				}

				if (event.payload.kind === 'quest') {
					overlayMenuKind = null;
					await facets.declareQuest(event.payload.id, event.payload.isPlaylist);
					return;
				}

				overlayMenuKind = null;
				await handleSelectMob({
					display: event.payload.maturity
						? `${event.payload.species} ${event.payload.maturity}`.trim()
						: event.payload.species,
					species: event.payload.species,
					maturity: event.payload.maturity
				});
			});

			unlistenClosed = await listen(OVERLAY_MENU_CLOSED_EVENT, async () => {
				if (disposed) return;
				overlayMenuKind = null;
				clearMobCloseTimer();
				facets.clearNameCloseTimer();
			});

			unlistenInteract = await listen(OVERLAY_MENU_INTERACT_EVENT, async () => {
				if (disposed) return;
				if (overlayMenuKind === 'mob') clearMobCloseTimer();
				if (overlayMenuKind === 'name') facets.clearNameCloseTimer();
			});
		})();

		return () => {
			disposed = true;
			unlistenSelect?.();
			unlistenClosed?.();
			unlistenInteract?.();
		};
	});

	$effect(() => {
		let disposed = false;
		let unlistenClosed: (() => void) | undefined;

		void (async () => {
			unlistenClosed = await listen(OVERLAY_ARMOUR_COST_CLOSED_EVENT, () => {
				if (disposed) return;
				armourCostClosedAt = Date.now();
				clearArmourCostOpenState();
			});
		})();

		return () => {
			disposed = true;
			unlistenClosed?.();
		};
	});



	// Map the one consolidated snapshot onto the overlay's two render bindings:
	// `data` (TrackingLive, the strip) and `status` (TrackingStatus, the stat
	// pills). TrackingSnapshot is a strict superset of TrackingStatus, so `status`
	// takes it directly; `data` is mapped field by field, bridging the snapshot's
	// snake `session_id` / `kill_count` onto the live shape's camel `sessionId` /
	// `killCount`. The activity feed (`recentEvents`) is deliberately not mapped:
	// the overlay renders no feed.
	function applySnapshot(snap: TrackingSnapshot) {
		status = snap;
		data = {
			status: snap.status ?? 'idle',
			sessionId: snap.session_id,
			elapsed: snap.elapsed,
			killCount: snap.kill_count,
			cost: snap.cost,
			returns: snap.returns,
			pes: snap.pes,
			net: snap.net,
			returnRate: snap.returnRate,
			weaponAttribution: snap.weaponAttribution,
			repairOcrEnabled: snap.repairOcrEnabled,
			endOfSessionArmourReminderEnabled: snap.endOfSessionArmourReminderEnabled,
			sessionName: snap.sessionName,
			skillBoostPercent: snap.skillBoostPercent,
			currentMob: snap.currentMob,
			currentTool: snap.currentTool,
			currentActivity: snap.currentActivity,
			questName: snap.questName,
			trifectaAttribution: snap.trifectaAttribution,
			harvestGuardrail: snap.harvestGuardrail,
		};
		const startedMs = snap.started_at ? new Date(snap.started_at).getTime() : NaN;
		sessionStartedAtMs = Number.isNaN(startedMs) ? null : startedMs;
	}

	$effect(() => {
		const current = snapshot.current;
		if (!current) return;
		applySnapshot(current);
	});

	const isTrifectaAttribution = $derived(data.weaponAttribution === 'trifecta');

	const armourSessionId = $derived(data.sessionId ?? flow.lastSessionId);
	const showManualInput = $derived(
		(data.status === 'active' || data.status === 'idle') && !data.currentMob
	);
	// The name is session-grain, so it takes input only while idle: once a
	// session runs, changing it could only rewrite the whole session's
	// history, and the correction path is the session record.
	const showNameInput = $derived(data.status === 'idle' && !data.sessionName);

	// Two independent debounced typeaheads, one per free-text facet: the
	// declared mob over the catalogue, the session name over the names
	// already in the history. They no longer share an endpoint because the
	// facets no longer exclude one another. Search failures are mapped to
	// the overlay's established wording before the typeahead records them.
	const mobTypeahead = createTypeahead<ManualMobSuggestion>({
		search: async (query) => {
			try {
				return await getManualMobSuggestions(query);
			} catch (error) {
				throw new Error(error instanceof ApiError ? error.message : 'Mob lookup failed');
			}
		},
		debounceMs: 120,
		minLength: 1
	});
	const mobSuggestions = $derived(mobTypeahead.results);
	const mobLoading = $derived(mobTypeahead.loading);

	// Drive each typeahead from its input state. Hiding the input or
	// emptying the query suspends the search and closes that menu, keeping
	// the typed text.
	$effect(() => {
		if (!showManualInput) {
			mobTypeahead.cancel();
			void closeMobMenu();
			overlayMenuLaunchError = null;
			return;
		}

		mobTypeahead.query = mobQuery;
		if (!mobQuery.trim()) {
			mobTypeahead.cancel();
			void closeMobMenu();
			overlayMenuLaunchError = null;
			return;
		}

		mobTypeahead.refresh();
	});

	$effect(() => {
		void facets.nameQuery;
		facets.syncNameQuery(showNameInput);
	});

	// Present the search lifecycle in the menu window: mirror the typeahead's
	// settled error into the shared channel and re-sync the menu at each
	// transition (the loading flip, a results publication, an error) while the
	// input is focused or the menu already open. Only the lifecycle is
	// tracked; the gate reads are untracked so a bare focus change cannot
	// re-open a menu with nothing new to show.
	$effect(() => {
		void mobTypeahead.loading;
		void mobTypeahead.results;
		mobError = mobTypeahead.error;
		untrack(() => {
			if (!showManualInput || !mobQuery.trim()) return;
			if (mobInputFocused || overlayMenuKind === 'mob') {
				void openMobMenu();
			}
		});
	});

	$effect(() => {
		void facets.nameLoading;
		void facets.nameSuggestions;
		untrack(() => facets.presentNameMenu(showNameInput, overlayMenuKind === 'name'));
	});

	$effect(() => {
		return () => {
			mobTypeahead.destroy();
			facets.destroy();
		};
	});

	// Keep the boost buffer in step with the persisted facet while the user
	// is not editing it (an idle overlay re-read, or the value the last
	// session left behind).
	$effect(() => {
		void data.skillBoostPercent;
		untrack(() => facets.syncBoostDraft());
	});


	async function handleStart() {
		starting = true;
		attributionWarning = null;
		try {
			await startTracking();
			await snapshot.hydrate();
		} catch (error) {
			if (error instanceof ApiError && error.kind === 'badRequest') {
				attributionWarning = error.message;
			}
		}
		starting = false;
	}

	function dismissAttributionWarning() {
		attributionWarning = null;
	}

	async function handleReleaseMob() {
		releasing = true;
		try {
			await releaseMob();
			mobQuery = '';
			mobTypeahead.cancel();
			await closeMobMenu();
			mobError = null;
			await snapshot.hydrate();
		} catch { /* ignore */ }
		releasing = false;
	}

	function handleMobFocus() {
		clearMobCloseTimer();
		mobInputFocused = true;
		if (mobQuery.trim() && (mobSuggestions.length > 0 || mobLoading || !!mobError)) {
			void openMobMenu();
		}
	}

	function handleMobBlur() {
		mobInputFocused = false;
		clearMobCloseTimer();
		mobCloseTimer = setTimeout(() => {
			void closeMobMenu();
		}, 120);
	}

	async function handleMobKeydown(event: KeyboardEvent) {
		if (event.key === 'Escape') {
			await closeMobMenu();
			return;
		}
		if (event.key !== 'Enter') return;

		// Only a catalogue mob can be declared, so Enter takes the top
		// match rather than inventing a name the catalogue cannot resolve.
		if (mobSuggestions.length > 0) {
			event.preventDefault();
			await handleSelectMob(mobSuggestions[0]);
		}
	}

	async function handleSelectMob(option: ManualMobSuggestion) {
		clearMobCloseTimer();
		selectingMob = true;
		mobError = null;
		try {
			await lockManualMob(option.species, option.maturity);
			mobQuery = '';
			mobTypeahead.cancel();
			overlayMenuLaunchError = null;
			await closeMobMenu();
			await snapshot.hydrate();
		} catch (error) {
			mobError = error instanceof ApiError ? error.message : 'Failed to declare mob';
		}
		selectingMob = false;
	}
</script>

<!-- Kept: mousedown is the frameless overlay window's drag handle (pointer-only by nature); the controls inside are native buttons. -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="p-2 flex flex-col items-start overlay-frame w-max" bind:this={overlayRoot} onmousedown={handleDrag}>
	<OverlayStrip
		{data}
		{status}
		{toggling}
		{releasing}
		{selectingMob}
		{trifectaSaving}
		{trifectaError}
		{armourCostOpen}
		{armourCostError}
		{armourSessionId}
		mobMenuOpen={overlayMenuKind === 'mob'}
		nameMenuOpen={overlayMenuKind === 'name'}
		questMenuOpen={overlayMenuKind === 'quest'}
		trifectaMenuOpen={overlayMenuKind === 'trifecta'}
		{overlayMenuLaunchError}
		savingName={facets.savingName}
		nameEditable={facets.nameEditable}
		savingBoost={facets.savingBoost}
		questSaving={facets.questSaving}
		facetError={facets.facetError}
		questLabel={data.questName ?? null}
		lastSessionId={flow.lastSessionId}
		lastSessionStats={flow.lastSessionStats}
		questLinkSuggestion={flow.questLinkSuggestion}
		questLinkMessage={flow.questLinkMessage}
		questLinkSaving={flow.questLinkSaving}
		bind:mobQuery
		bind:mobInput
		bind:nameQuery={facets.nameQuery}
		bind:nameInput={facets.nameInput}
		bind:boostDraft={facets.boostDraft}
		bind:postSessionArmourButton
		onStart={handleStart}
		onStop={flow.requestStop}
		awaitingArmourTrackDecision={flow.awaitingArmourDecision}
		onArmourTrackDecision={flow.decideArmourTrack}
		attributionWarning={attributionWarning}
		onDismissAttributionWarning={dismissAttributionWarning}
		onReleaseMob={handleReleaseMob}
		onMobFocus={handleMobFocus}
		onMobBlur={handleMobBlur}
		onMobKeydown={handleMobKeydown}
		onNameFocus={facets.handleNameFocus}
		onNameBlur={facets.handleNameBlur}
		onNameKeydown={facets.handleNameKeydown}
		onClearName={facets.clearName}
		onBoostCommit={facets.commitBoost}
		onQuestTrigger={toggleQuestMenu}
		onClearQuest={facets.clearQuest}
		onTrifectaTrigger={toggleTrifectaMenu}
		onArmourCostToggle={toggleArmourCost}
		onQuestLinkDecision={flow.decideQuestLink}
		onDismissQuestLinkMessage={flow.dismissQuestLinkMessage}
	/>
</div>

<style>
	.overlay-frame {
		overflow: visible;
	}
</style>
