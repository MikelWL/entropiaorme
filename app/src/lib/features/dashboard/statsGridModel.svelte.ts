/**
 * Dashboard stats-grid view model: the enabled-stat projection, the
 * pointer-driven drag-reorder, and the guide-mode stat demo controls.
 * The only consumer of the stat-customisation state on the dashboard
 * surface; presentation composes over this state.
 */

import {
	DEFAULT_OVERLAY_PREFS,
	DEFAULT_STAT_PREFS,
	dashboardStats,
	overlayStats,
	type StatPref,
	setDashboardStats,
} from '$lib/statsCustomisation.svelte';

export interface StatsSnapshot {
	dashboard: StatPref[];
	overlay: StatPref[];
}

// Preselected stat configuration applied to both stores while the guide
// is open. Cards 1-6 show populated stat content (10 dashboard + 3
// overlay pills enabled) regardless of the live prefs. Card 7
// (modular-stats) takes its own snapshot at play() start and switches
// to a 3-enabled baseline for the demo, then restores this preselected
// configuration on exit, so back-nav from card 7 to 6 lands cleanly.
// Guide-close reverses the outer snapshot to restore the live stats.
const DASHBOARD_GUIDE_PRESELECTED_IDS = new Set<string>([
	'cycled',
	'loot_tt',
	'net',
	'rate',
	'pes',
	'pes_per_100',
	'avg_cost_per_kill',
	'multiplier_max',
	'dpp',
	'kills_count',
]);
const OVERLAY_GUIDE_PRESELECTED_IDS = new Set<string>(['cycled', 'rate', 'kills_count']);
const DASHBOARD_GUIDE_PRESELECTED: StatPref[] = DEFAULT_STAT_PREFS.map((p) => ({
	id: p.id,
	enabled: DASHBOARD_GUIDE_PRESELECTED_IDS.has(p.id),
}));
const OVERLAY_GUIDE_PRESELECTED: StatPref[] = DEFAULT_STAT_PREFS.map((p) => ({
	id: p.id,
	enabled: OVERLAY_GUIDE_PRESELECTED_IDS.has(p.id),
}));

const REORDER_COOLDOWN_MS = 100;
const DRAG_THRESHOLD_PX = 4;

function fullIndexOfEnabled(prefs: StatPref[], filteredIndex: number): number {
	let count = 0;
	for (let i = 0; i < prefs.length; i++) {
		if (prefs[i].enabled) {
			if (count === filteredIndex) return i;
			count++;
		}
	}
	return -1;
}

export function createStatsGridModel() {
	// Stats grid drag-reorder via pointer events (not HTML5 drag: the latter
	// cedes cursor control to the OS, so we can't keep the grabbing hand stable
	// through the gesture). dragFilteredIndex tracks the dragged cell's position
	// within the enabled-only filtered list; the underlying full store list is
	// mutated via fullIndexOfEnabled() so disabled stats stay in their slots.
	let dragFilteredIndex = $state<number | null>(null);
	let dragMoved = $state(false);
	let dragStartX = 0;
	let dragStartY = 0;
	// Cooldown after each reorder so cursor jitter at a cell boundary doesn't
	// ping-pong the layout while the flip animation is still settling.
	let lastReorderAt = 0;

	// Snapshot of the live prefs held while the guide's preselected demo
	// configuration is applied; undefined means "no snapshot held".
	let guideSnapshot: StatsSnapshot | undefined;

	function handlePointerDown(e: PointerEvent, filteredIndex: number) {
		if (e.button !== 0) return;
		const target = e.currentTarget as HTMLElement;
		target.setPointerCapture(e.pointerId);
		dragFilteredIndex = filteredIndex;
		dragStartX = e.clientX;
		dragStartY = e.clientY;
		dragMoved = false;
		lastReorderAt = 0;
		document.body.classList.add('stat-drag-active');
	}

	function handlePointerMove(e: PointerEvent) {
		if (dragFilteredIndex === null) return;
		// Threshold-gate: don't reorder for sub-pixel jitter on a click.
		if (!dragMoved) {
			const dx = e.clientX - dragStartX;
			const dy = e.clientY - dragStartY;
			if (dx * dx + dy * dy < DRAG_THRESHOLD_PX * DRAG_THRESHOLD_PX) return;
			dragMoved = true;
		}
		const now = performance.now();
		if (now - lastReorderAt < REORDER_COOLDOWN_MS) return;
		// Hit-test by walking cells' bounding rects directly. elementFromPoint
		// would return the captured (dragged) element because of pointer capture.
		const cells = document.querySelectorAll<HTMLElement>('[data-stat-cell]');
		let targetFilteredIndex = -1;
		for (const cell of cells) {
			const rect = cell.getBoundingClientRect();
			if (
				e.clientX >= rect.left &&
				e.clientX <= rect.right &&
				e.clientY >= rect.top &&
				e.clientY <= rect.bottom
			) {
				const idx = Number(cell.dataset.statCell);
				if (!Number.isNaN(idx)) targetFilteredIndex = idx;
				break;
			}
		}
		if (targetFilteredIndex < 0 || targetFilteredIndex === dragFilteredIndex) return;
		const full = dashboardStats.current;
		const sourceFull = fullIndexOfEnabled(full, dragFilteredIndex);
		const targetFull = fullIndexOfEnabled(full, targetFilteredIndex);
		if (sourceFull < 0 || targetFull < 0) return;
		const next = [...full];
		const [moved] = next.splice(sourceFull, 1);
		next.splice(targetFull, 0, moved);
		dashboardStats.current = next;
		dragFilteredIndex = targetFilteredIndex;
		lastReorderAt = now;
	}

	function handlePointerUp(e: PointerEvent) {
		if (dragFilteredIndex === null) return;
		const target = e.currentTarget as HTMLElement;
		if (target?.hasPointerCapture?.(e.pointerId)) {
			target.releasePointerCapture(e.pointerId);
		}
		if (dragMoved) void setDashboardStats(dashboardStats.current);
		dragFilteredIndex = null;
		dragMoved = false;
		lastReorderAt = 0;
		document.body.classList.remove('stat-drag-active');
	}

	function handlePointerCancel() {
		if (dragFilteredIndex === null) return;
		if (dragMoved) void setDashboardStats(dashboardStats.current);
		dragFilteredIndex = null;
		dragMoved = false;
		lastReorderAt = 0;
		document.body.classList.remove('stat-drag-active');
	}

	// ── Guide-mode demo controls ──

	function snapshotStats(): StatsSnapshot {
		return {
			dashboard: dashboardStats.current,
			overlay: overlayStats.current,
		};
	}

	function restoreStats(snap: StatsSnapshot) {
		dashboardStats.current = snap.dashboard;
		overlayStats.current = snap.overlay;
	}

	function setDemoStatsBaseline(overrides?: Record<string, boolean>) {
		// Reset both surfaces to default prefs (transient: no setDashboardStats
		// call, so nothing persists to user preferences). Optional overrides
		// flip specific stat-ids' enabled flags, letting cards bend the
		// baseline (e.g. start the modular-stats card with 3 enabled
		// instead of 4) without forking the constant.
		const base = overrides
			? DEFAULT_STAT_PREFS.map((p) => (p.id in overrides ? { ...p, enabled: overrides[p.id] } : p))
			: DEFAULT_STAT_PREFS;
		dashboardStats.current = base;
		overlayStats.current = DEFAULT_OVERLAY_PREFS;
	}

	function toggleDemoStatPill(surface: 'dashboard' | 'overlay', statId: string) {
		// Transient toggle on the named pill. Mirrors handlePillClick's
		// shape (map then flip enabled flag) but bypasses setDashboardStats
		// so the persisted prefs aren't touched.
		const target = surface === 'dashboard' ? dashboardStats : overlayStats;
		const next = target.current.map((p) => (p.id === statId ? { ...p, enabled: !p.enabled } : p));
		target.current = next;
	}

	function reorderDemoStat(fromFilteredIdx: number, toFilteredIdx: number) {
		// Transient reorder using the existing fullIndexOfEnabled logic
		// so the move respects the disabled-stats-stay-put invariant
		// the real drag handler enforces.
		const current = dashboardStats.current;
		const sourceFull = fullIndexOfEnabled(current, fromFilteredIdx);
		const targetFull = fullIndexOfEnabled(current, toFilteredIdx);
		if (sourceFull < 0 || targetFull < 0) return;
		const next = [...current];
		const [moved] = next.splice(sourceFull, 1);
		next.splice(targetFull, 0, moved);
		dashboardStats.current = next;
	}

	function setDragVisualIndex(idx: number | null) {
		// Sets the drag state directly so the real drag visual (opacity-40 +
		// shadow + ring on the cell at the matching filtered index) renders
		// for the guide's virtual drag.
		dragFilteredIndex = idx;
	}

	function syncGuideStats(active: boolean) {
		// Snapshot the live config on guide-open + apply the preselected demo
		// configuration so cards 1-6 render populated stats grids. Card 7
		// takes its own snapshot at play() start (which captures this
		// preselected config) and runs its own 3-enabled baseline demo on top,
		// restoring the preselected on its exit so back-nav from 7 to 6 is
		// clean. Guide-close reverses this outer snapshot to restore the live
		// config.
		if (active && guideSnapshot === undefined) {
			guideSnapshot = snapshotStats();
			dashboardStats.current = DASHBOARD_GUIDE_PRESELECTED;
			overlayStats.current = OVERLAY_GUIDE_PRESELECTED;
		} else if (!active && guideSnapshot !== undefined) {
			restoreStats(guideSnapshot);
			guideSnapshot = undefined;
		}
	}

	return {
		/** The enabled dashboard prefs, in stored order: the grid's render list. */
		get enabledStats() {
			return dashboardStats.current.filter((p) => p.enabled);
		},
		get dragFilteredIndex() {
			return dragFilteredIndex;
		},

		handlePointerDown,
		handlePointerMove,
		handlePointerUp,
		handlePointerCancel,

		snapshotStats,
		restoreStats,
		setDemoStatsBaseline,
		toggleDemoStatPill,
		reorderDemoStat,
		setDragVisualIndex,
		syncGuideStats,
	};
}

export type StatsGridModel = ReturnType<typeof createStatsGridModel>;
