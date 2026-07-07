/**
 * The settings family, plus the overlay-position persistence. Thin
 * wrappers over the generated typed commands.
 */

import type { Hotbar } from '$lib/types/settings';
import type { AppSettings } from './commands.gen';
import * as commands from './commands.gen';

export const getSettings = commands.settingsGet;
export const updateSettings = commands.settingsUpdate;
export const getOverlayPosition = commands.settingsOverlayPosition;
export const saveOverlayPosition = commands.settingsSetOverlayPosition;

/** The settings response's hotbar block as the typed slot map. The wire
 * carries the stored JSON verbatim (`Record<string, unknown>`); slots are
 * written as equipment ids or null, so anything else reads as an empty
 * slot rather than a fabricated binding. */
export function hotbarFromSettings(settings: AppSettings): Hotbar {
	const hotbar: Hotbar = {};
	for (const [slot, value] of Object.entries(settings.hotbar)) {
		hotbar[slot] = typeof value === 'number' ? value : null;
	}
	return hotbar;
}
