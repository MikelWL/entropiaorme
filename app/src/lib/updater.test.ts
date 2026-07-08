// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// Mock the Tauri IPC + event seams and the preferences seam so the flow is
// observable without a running backend.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn() }));
vi.mock('./preferences', () => ({
	getPreference: vi.fn(),
	setPreference: vi.fn(),
}));

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getPreference, setPreference } from './preferences';
import {
	AUTO_UPDATE_PREFERENCE_KEY,
	autoUpdateEnabled,
	availableUpdate,
	checkForUpdate,
	downloadUpdate,
	initUpdater,
	maybeCheckOnLaunch,
	setAutoUpdateEnabled,
	showUpdateToast,
	type UpdateInfo,
	updateAvailable,
	updatePhase,
	updateToastDismissed,
} from './updater.svelte';

const invokeMock = vi.mocked(invoke);
const listenMock = vi.mocked(listen);
const getPreferenceMock = vi.mocked(getPreference);
const setPreferenceMock = vi.mocked(setPreference);

function withTauri(): void {
	(window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
}
function withoutTauri(): void {
	delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
}

const sampleUpdate: UpdateInfo = { version: '0.2.0', currentVersion: '0.1.0', notes: 'Fixes.' };

beforeEach(() => {
	autoUpdateEnabled.current = false;
	updatePhase.current = 'idle';
	availableUpdate.current = null;
	updateToastDismissed.current = false;
	invokeMock.mockReset();
	listenMock.mockReset();
	listenMock.mockResolvedValue(() => {});
	getPreferenceMock.mockReset();
	setPreferenceMock.mockReset();
	setPreferenceMock.mockResolvedValue(undefined);
	withTauri();
});

afterEach(() => {
	withoutTauri();
	vi.clearAllMocks();
});

describe('initUpdater', () => {
	it('loads auto-update OFF at runtime until chosen (the opt-out posture is panel-driven)', async () => {
		// The runtime state stays OFF until the user has made the choice, so the
		// launch check never fires before consent; the opt-out "on by default"
		// lives in the onboarding panel and the saved preference.
		getPreferenceMock.mockImplementation(
			async (_key: string, defaultValue: unknown) => defaultValue,
		);

		await initUpdater();

		expect(getPreferenceMock).toHaveBeenCalledWith(AUTO_UPDATE_PREFERENCE_KEY, false);
		expect(autoUpdateEnabled.current).toBe(false);
	});

	it('honours a persisted opt-out', async () => {
		getPreferenceMock.mockResolvedValue(false);

		await initUpdater();

		expect(autoUpdateEnabled.current).toBe(false);
	});
});

describe('setAutoUpdateEnabled', () => {
	it('sets the state and persists the choice', async () => {
		await setAutoUpdateEnabled(false);

		expect(autoUpdateEnabled.current).toBe(false);
		expect(setPreferenceMock).toHaveBeenCalledWith(AUTO_UPDATE_PREFERENCE_KEY, false);
	});
});

describe('checkForUpdate', () => {
	it('marks an available update and clears any prior toast dismissal', async () => {
		updateToastDismissed.current = true;
		invokeMock.mockResolvedValue(sampleUpdate);

		const result = await checkForUpdate();

		expect(invokeMock).toHaveBeenCalledWith('check_for_update');
		expect(result).toEqual(sampleUpdate);
		expect(updatePhase.current).toBe('available');
		expect(availableUpdate.current).toEqual(sampleUpdate);
		expect(updateToastDismissed.current).toBe(false);
	});

	it('marks up-to-date when the backend reports no update', async () => {
		invokeMock.mockResolvedValue(null);

		await checkForUpdate();

		expect(updatePhase.current).toBe('up-to-date');
		expect(availableUpdate.current).toBeNull();
	});

	it('surfaces an error phase when the check throws', async () => {
		invokeMock.mockRejectedValue('offline');

		await checkForUpdate();

		expect(updatePhase.current).toBe('error');
	});

	it('is a no-op outside Tauri', async () => {
		withoutTauri();

		const result = await checkForUpdate();

		expect(result).toBeNull();
		expect(invokeMock).not.toHaveBeenCalled();
		expect(updatePhase.current).toBe('idle');
	});
});

describe('downloadUpdate', () => {
	it('subscribes to progress, downloads, and reaches the ready phase', async () => {
		invokeMock.mockResolvedValue(sampleUpdate);

		await downloadUpdate();

		expect(listenMock).toHaveBeenCalledWith('updater:download-progress', expect.any(Function));
		expect(invokeMock).toHaveBeenCalledWith('download_update');
		expect(updatePhase.current).toBe('ready');
	});

	it('surfaces an error phase when the download throws', async () => {
		invokeMock.mockRejectedValue('signature mismatch');

		await downloadUpdate();

		expect(updatePhase.current).toBe('error');
	});
});

describe('maybeCheckOnLaunch', () => {
	it('checks when auto-update is enabled', async () => {
		autoUpdateEnabled.current = true;
		invokeMock.mockResolvedValue(null);

		await maybeCheckOnLaunch();

		expect(invokeMock).toHaveBeenCalledWith('check_for_update');
	});

	it('does nothing when the user has opted out', async () => {
		autoUpdateEnabled.current = false;

		await maybeCheckOnLaunch();

		expect(invokeMock).not.toHaveBeenCalled();
	});

	it('stays silent on failure (no error phase) for the launch check', async () => {
		autoUpdateEnabled.current = true;
		invokeMock.mockRejectedValue('offline');

		await maybeCheckOnLaunch();

		// A failed launch check must not leave /updates in an error state.
		expect(updatePhase.current).toBe('idle');
	});
});

describe('derived state', () => {
	it('updateAvailable tracks the pending phases', () => {
		updatePhase.current = 'idle';
		expect(updateAvailable.current).toBe(false);
		updatePhase.current = 'available';
		expect(updateAvailable.current).toBe(true);
		updatePhase.current = 'ready';
		expect(updateAvailable.current).toBe(true);
		updatePhase.current = 'up-to-date';
		expect(updateAvailable.current).toBe(false);
	});

	it('showUpdateToast is suppressed by dismissal', () => {
		updatePhase.current = 'available';
		updateToastDismissed.current = false;
		expect(showUpdateToast.current).toBe(true);
		updateToastDismissed.current = true;
		expect(showUpdateToast.current).toBe(false);
	});
});
