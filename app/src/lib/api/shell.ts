/**
 * The shell-surface commands: the handful of bespoke window and byte
 * commands the Tauri shell registers outside the typed eo-api manifest
 * (window lifecycle is the shell's, not the facade's, and the capture
 * preview answers raw bytes rather than a JSON DTO). This module is the
 * single home for their invoke strings; nothing else in the frontend
 * calls `invoke` with a bare command name.
 *
 * Their error contract is the shell's (a plain string or a plugin
 * error), not `ApiErrorPayload`, so they ride `invoke` directly rather
 * than the typed transport in `./invoke`.
 */

import { invoke } from '@tauri-apps/api/core';

/** Toggle the pre-spawned tracking overlay window's visibility. */
export async function toggleOverlay(): Promise<void> {
	await invoke('toggle_overlay');
}

/** Show and focus the pre-spawned scan overlay window. */
export async function showScanOverlay(): Promise<void> {
	await invoke('show_scan_overlay');
}

/** Hide the scan overlay window. */
export async function hideScanOverlay(): Promise<void> {
	await invoke('hide_scan_overlay');
}

/** Metadata about an available update (mirrors the Rust `UpdateInfo`). */
export type UpdateInfo = {
	version: string;
	currentVersion: string;
	notes: string | null;
};

/** Ask the updater to check the release manifest; null when current. */
export async function checkForUpdate(): Promise<UpdateInfo | null> {
	return invoke('check_for_update');
}

/** Download the available update; resolves once staged. */
export async function downloadUpdate(): Promise<UpdateInfo> {
	return invoke('download_update');
}

/** The configured update channel. */
export async function getUpdateChannel(): Promise<string> {
	return invoke('get_update_channel');
}

/** Hand off to the updater: install the staged update and restart. */
export async function installUpdate(): Promise<void> {
	await invoke('install_update');
}

/** The manual-scan capture preview PNG for a page, as a base64 `data:`
 * URL for an `<img>` `src`. */
export async function manualSkillScanCapturePng(page: number): Promise<string> {
	const encoded = await invoke<string>('capture_png', { page });
	return `data:image/png;base64,${encoded}`;
}

/** A bundled planet map's raster as a base64 `data:` URL for an `<img>`
 * `src`. `mime` comes from the planet's `planet_maps_list` record. */
export async function planetMapImage(planet: string, mime: string): Promise<string> {
	const encoded = await invoke<string>('planet_map_image', { planet });
	return `data:${mime};base64,${encoded}`;
}
