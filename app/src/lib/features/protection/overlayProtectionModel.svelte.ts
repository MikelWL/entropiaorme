import {
	ApiError,
	getProtectionOverview,
	type ProtectionOverview,
	selectProtectionLoadout,
} from '$lib/api';
import { inDevelopment } from '$lib/inDevelopment';

export function createOverlayProtectionModel(refreshTracking: () => Promise<unknown>) {
	let overview = $state<ProtectionOverview | null>(null);
	let saving = $state(false);
	let error = $state<string | null>(null);

	async function refresh(): Promise<void> {
		if (!inDevelopment.visible) return;
		try {
			overview = await getProtectionOverview();
			error = null;
		} catch (cause) {
			error = cause instanceof ApiError ? cause.message : 'Protection setup failed to load';
		}
	}

	async function select(id: string): Promise<void> {
		if (saving || overview?.activeLoadoutId === id) return;
		saving = true;
		error = null;
		try {
			overview = await selectProtectionLoadout(id);
			await refreshTracking();
		} catch (cause) {
			error = cause instanceof ApiError ? cause.message : 'Protection selection failed';
		} finally {
			saving = false;
		}
	}

	return {
		get overview() {
			return inDevelopment.visible ? overview : null;
		},
		get saving() {
			return saving;
		},
		get error() {
			return error;
		},
		refresh,
		select,
	};
}
