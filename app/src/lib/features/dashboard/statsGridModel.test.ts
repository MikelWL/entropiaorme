// @vitest-environment happy-dom

import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
	DEFAULT_OVERLAY_PREFS,
	DEFAULT_STAT_PREFS,
	dashboardStats,
	overlayStats,
	type StatPref,
} from '$lib/statsCustomisation';
import type { StatId } from '$lib/statsRegistry';
import { readLegacyStore } from '$lib/view/legacyStore.svelte';
import { createStatsGridModel } from './statsGridModel.svelte';

vi.mock('$lib/preferences', () => ({
	getPreference: vi.fn(),
	setPreference: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('@tauri-apps/api/event', () => ({
	emit: vi.fn().mockResolvedValue(undefined),
	listen: vi.fn(),
}));

import { setPreference } from '$lib/preferences';

function prefsWith(enabled: StatId[]): StatPref[] {
	return DEFAULT_STAT_PREFS.map((p) => ({ id: p.id, enabled: enabled.includes(p.id) }));
}

function enabledIds(): string[] {
	return readLegacyStore(dashboardStats)
		.filter((p) => p.enabled)
		.map((p) => p.id);
}

/** A minimal pointer-event stand-in carrying only the fields the handlers read. */
function pointerEvent(
	target: EventTarget,
	overrides: Partial<{ button: number; clientX: number; clientY: number }> = {},
): PointerEvent {
	return {
		button: 0,
		pointerId: 1,
		clientX: 0,
		clientY: 0,
		currentTarget: target,
		...overrides,
	} as unknown as PointerEvent;
}

function cellTarget() {
	return {
		setPointerCapture: vi.fn(),
		hasPointerCapture: vi.fn().mockReturnValue(true),
		releasePointerCapture: vi.fn(),
	} as unknown as HTMLElement;
}

/** Mount fake grid cells whose bounding rects tile a 100px-tall column. */
function mountCells(count: number) {
	document.body.innerHTML = '';
	for (let i = 0; i < count; i++) {
		const cell = document.createElement('div');
		cell.dataset.statCell = String(i);
		cell.getBoundingClientRect = () =>
			({ left: 0, right: 100, top: i * 100, bottom: (i + 1) * 100 }) as DOMRect;
		document.body.appendChild(cell);
	}
}

beforeEach(() => {
	vi.clearAllMocks();
	document.body.innerHTML = '';
	document.body.classList.remove('stat-drag-active');
	dashboardStats.set(DEFAULT_STAT_PREFS);
	overlayStats.set(DEFAULT_OVERLAY_PREFS);
});

describe('enabledStats', () => {
	it('projects only the enabled prefs, in stored order', () => {
		dashboardStats.set(prefsWith(['net', 'cycled']));
		const model = createStatsGridModel();
		expect(model.enabledStats.map((p) => p.id)).toEqual(['cycled', 'net']);
	});
});

describe('drag reorder', () => {
	it('moves the dragged stat past the threshold while disabled stats keep their slots', () => {
		dashboardStats.set(prefsWith(['cycled', 'net', 'rate'])); // loot_tt stays disabled between them
		mountCells(3);
		const model = createStatsGridModel();
		const target = cellTarget();

		model.handlePointerDown(pointerEvent(target, { clientX: 50, clientY: 50 }), 0);
		expect(document.body.classList.contains('stat-drag-active')).toBe(true);
		expect(model.dragFilteredIndex).toBe(0);

		// Drop the first enabled stat (cycled) onto the last cell.
		model.handlePointerMove(pointerEvent(target, { clientX: 50, clientY: 250 }));
		expect(enabledIds()).toEqual(['net', 'rate', 'cycled']);
		expect(model.dragFilteredIndex).toBe(2);

		model.handlePointerUp(pointerEvent(target, { clientX: 50, clientY: 250 }));
		expect(model.dragFilteredIndex).toBeNull();
		expect(document.body.classList.contains('stat-drag-active')).toBe(false);
		// A real move persists through the canonical setter.
		expect(setPreference).toHaveBeenCalled();
	});

	it('treats sub-threshold jitter as a click: no reorder, no persist', () => {
		dashboardStats.set(prefsWith(['cycled', 'net', 'rate']));
		mountCells(3);
		const model = createStatsGridModel();
		const target = cellTarget();

		model.handlePointerDown(pointerEvent(target, { clientX: 50, clientY: 50 }), 0);
		model.handlePointerMove(pointerEvent(target, { clientX: 52, clientY: 51 }));
		expect(enabledIds()).toEqual(['cycled', 'net', 'rate']);

		model.handlePointerUp(pointerEvent(target, { clientX: 52, clientY: 51 }));
		expect(setPreference).not.toHaveBeenCalled();
	});

	it('ignores non-primary buttons and restores cleanly on cancel', () => {
		dashboardStats.set(prefsWith(['cycled', 'net']));
		mountCells(2);
		const model = createStatsGridModel();
		const target = cellTarget();

		model.handlePointerDown(pointerEvent(target, { button: 2 }), 0);
		expect(model.dragFilteredIndex).toBeNull();

		model.handlePointerDown(pointerEvent(target, { clientX: 50, clientY: 50 }), 0);
		model.handlePointerCancel();
		expect(model.dragFilteredIndex).toBeNull();
		expect(document.body.classList.contains('stat-drag-active')).toBe(false);
	});
});

describe('guide demo controls', () => {
	it('toggles a pill transiently without persisting', () => {
		const model = createStatsGridModel();
		model.toggleDemoStatPill('dashboard', 'pes');
		expect(enabledIds()).toContain('pes');
		model.toggleDemoStatPill('overlay', 'cycled');
		expect(setPreference).not.toHaveBeenCalled();
	});

	it('resets to the default baseline with optional overrides', () => {
		dashboardStats.set(prefsWith(['pes']));
		const model = createStatsGridModel();
		model.setDemoStatsBaseline({ rate: false });
		expect(enabledIds()).toEqual(['cycled', 'loot_tt', 'net']);
		model.setDemoStatsBaseline();
		expect(enabledIds()).toEqual(['cycled', 'loot_tt', 'net', 'rate']);
	});

	it('reorders transiently through the filtered indices', () => {
		dashboardStats.set(prefsWith(['cycled', 'net', 'rate']));
		const model = createStatsGridModel();
		model.reorderDemoStat(2, 0);
		expect(enabledIds()).toEqual(['rate', 'cycled', 'net']);
		// Out-of-range indices are a no-op.
		model.reorderDemoStat(9, 0);
		expect(enabledIds()).toEqual(['rate', 'cycled', 'net']);
	});

	it('drives the drag visual index for the virtual drag', () => {
		const model = createStatsGridModel();
		model.setDragVisualIndex(3);
		expect(model.dragFilteredIndex).toBe(3);
		model.setDragVisualIndex(null);
		expect(model.dragFilteredIndex).toBeNull();
	});

	it('snapshots on guide-open, applies the preselected set, and restores on close', () => {
		dashboardStats.set(prefsWith(['pes']));
		const model = createStatsGridModel();

		model.syncGuideStats(true);
		expect(enabledIds()).toHaveLength(10);
		expect(readLegacyStore(overlayStats).filter((p) => p.enabled)).toHaveLength(3);

		// Re-entrant active sync keeps the original snapshot.
		model.syncGuideStats(true);
		model.syncGuideStats(false);
		expect(enabledIds()).toEqual(['pes']);

		// A second close with no snapshot held is a no-op.
		model.syncGuideStats(false);
		expect(enabledIds()).toEqual(['pes']);
	});

	it('round-trips an explicit snapshot/restore pair', () => {
		const model = createStatsGridModel();
		const snap = model.snapshotStats();
		model.toggleDemoStatPill('dashboard', 'cycled');
		model.restoreStats(snap);
		expect(enabledIds()).toEqual(['cycled', 'loot_tt', 'net', 'rate']);
	});
});
