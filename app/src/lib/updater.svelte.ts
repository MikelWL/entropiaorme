// Auto-update client state and command wrappers.
//
// The app reaches the updater through Rust commands (not the JS updater plugin
// directly), so channel resolution and the forced-exit teardown live on the
// Rust side; this module is the thin frontend seam: the opt-out preference, the
// check / download / install flow, the download-progress subscription, and the
// derived state the toast and the Updates page read.
//
// Networking posture: the launch-time check is an outbound call, so it is gated
// on the auto-update preference (default ON; the user opts out). It transmits no
// user data: a plain GET of a static per-channel manifest, the running version
// is compared locally, nothing about the user is sent.

import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import {
	checkForUpdate as invokeCheckForUpdate,
	downloadUpdate as invokeDownloadUpdate,
	getUpdateChannel as invokeGetUpdateChannel,
	installUpdate as invokeInstallUpdate,
	type UpdateInfo,
} from './api/shell';
import { getPreference, setPreference } from './preferences';

/// The auto-update preference key. The runtime default is OFF until the user
/// has made the choice; the opt-out posture (auto-update on by default, opted
/// out in onboarding) is carried by the onboarding panel's default-on toggle and
/// the saved preference, so the launch check never fires before consent.
const KEY_AUTO_UPDATE_ENABLED = 'auto_update_enabled';
export const AUTO_UPDATE_PREFERENCE_KEY = KEY_AUTO_UPDATE_ENABLED;

/// The download-progress event the Rust side emits (colon-form, matching the bus).
const DOWNLOAD_PROGRESS_EVENT = 'updater:download-progress';

export type { UpdateInfo };

/// Bytes-arrived progress (mirrors the Rust `DownloadProgress`). `contentLength`
/// is null when the server uses chunked transfer; the UI shows indeterminate.
export type DownloadProgress = {
	downloaded: number;
	contentLength: number | null;
};

/// The update flow's phase, driving every update surface.
export type UpdatePhase =
	| 'idle' // no check run, or the result was dismissed
	| 'checking'
	| 'up-to-date'
	| 'available' // a newer release exists, not yet downloaded
	| 'downloading'
	| 'ready' // downloaded, awaiting the user's install-and-restart
	| 'installing'
	| 'error';

let enabled = $state(false);
let phase = $state<UpdatePhase>('idle');
let available = $state<UpdateInfo | null>(null);
let progress = $state<DownloadProgress | null>(null);
let error = $state<string | null>(null);
// Session-scoped toast dismissal. Not persisted: per the re-nudge decision, a
// dismissed update stays silent only until the next launch check.
let toastDismissed = $state(false);

export const autoUpdateEnabled = {
	get current(): boolean {
		return enabled;
	},
	set current(value: boolean) {
		enabled = value;
	},
};

export const updatePhase = {
	get current(): UpdatePhase {
		return phase;
	},
	set current(value: UpdatePhase) {
		phase = value;
	},
};

export const availableUpdate = {
	get current(): UpdateInfo | null {
		return available;
	},
	set current(value: UpdateInfo | null) {
		available = value;
	},
};

export const downloadProgress = {
	get current(): DownloadProgress | null {
		return progress;
	},
	set current(value: DownloadProgress | null) {
		progress = value;
	},
};

export const updateError = {
	get current(): string | null {
		return error;
	},
	set current(value: string | null) {
		error = value;
	},
};

export const updateToastDismissed = {
	get current(): boolean {
		return toastDismissed;
	},
	set current(value: boolean) {
		toastDismissed = value;
	},
};

/// Whether an update is pending the user's attention (drives the sidebar dot).
export const updateAvailable = {
	get current(): boolean {
		return phase === 'available' || phase === 'downloading' || phase === 'ready';
	},
};

/// Whether to show the toast: an update is pending and not dismissed this session.
export const showUpdateToast = {
	get current(): boolean {
		return updateAvailable.current && !toastDismissed;
	},
};

/// Whether the Tauri IPC bridge is present. Checked at call time (not module
/// load) so the bridge can appear after import and so tests can toggle it.
function isTauri(): boolean {
	return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

let unlistenProgress: UnlistenFn | null = null;

/// Hydrate the opt-out preference from the store. Call once on app start.
export async function initUpdater(): Promise<void> {
	enabled = await getPreference<boolean>(KEY_AUTO_UPDATE_ENABLED, false);
}

/// Persist the opt-out preference.
export async function setAutoUpdateEnabled(value: boolean): Promise<void> {
	enabled = value;
	await setPreference(KEY_AUTO_UPDATE_ENABLED, value);
}

/// Check the active channel's manifest for a newer release.
export async function checkForUpdate(silent = false): Promise<UpdateInfo | null> {
	if (!isTauri()) return null;
	error = null;
	if (!silent) phase = 'checking';
	try {
		const info = await invokeCheckForUpdate();
		if (info) {
			available = info;
			phase = 'available';
			toastDismissed = false;
		} else {
			available = null;
			// A silent launch check leaves no trace when up to date; a manual
			// check reports it.
			phase = silent ? 'idle' : 'up-to-date';
		}
		return info;
	} catch (err) {
		// A silent launch check must not surface a transport failure: an offline
		// launch would otherwise open /updates in an error state the user never
		// asked for. Only a user-initiated check reports the error.
		if (silent) {
			phase = 'idle';
		} else {
			error = String(err);
			phase = 'error';
		}
		return null;
	}
}

/// Download the available update (verifying its signature) and hold it for
/// install, surfacing progress. Idempotent on the progress listener.
export async function downloadUpdate(): Promise<void> {
	if (!isTauri()) return;
	error = null;
	progress = null;
	phase = 'downloading';
	if (!unlistenProgress) {
		unlistenProgress = await listen<DownloadProgress>(DOWNLOAD_PROGRESS_EVENT, (event) => {
			progress = event.payload;
		});
	}
	try {
		const info = await invokeDownloadUpdate();
		available = info;
		phase = 'ready';
	} catch (err) {
		error = String(err);
		phase = 'error';
	}
}

/// Install the downloaded update and relaunch. On success the process exits
/// before this resolves (Windows), so only the failure path returns control.
export async function installUpdate(): Promise<void> {
	if (!isTauri()) return;
	error = null;
	phase = 'installing';
	try {
		await invokeInstallUpdate();
	} catch (err) {
		error = String(err);
		phase = 'error';
	}
}

/// The current update channel (read-only in the UI for the 0.x window: only
/// stable is surfaced, though the channel plumbing supports beta).
export async function getUpdateChannel(): Promise<string> {
	if (!isTauri()) return 'stable';
	try {
		return await invokeGetUpdateChannel();
	} catch {
		return 'stable';
	}
}

/// Dismiss the toast for this session.
export function dismissUpdateToast(): void {
	toastDismissed = true;
}

/// The launch-time check, gated on the opt-out preference. Silent on failure:
/// a failed update check must never disrupt startup.
export async function maybeCheckOnLaunch(): Promise<void> {
	if (!isTauri()) return;
	if (!enabled) return;
	await checkForUpdate(true);
}
