import { dataDir, join } from '@tauri-apps/api/path';
import { load, type Store } from '@tauri-apps/plugin-store';

const APP_DATA_FOLDER = 'EntropiaOrme';
// The e2e shell writes its preferences to a separate file so a test run can
// never read or mutate a real installation's preferences (onboarding state,
// consent choices) on the same machine. The flag is baked in at build time by
// the e2e's own Vite build only (see app/vite.config.ts), so shipped builds
// fold this to the plain 'settings.json'.
const STORE_FILE =
	import.meta.env.E2E_ISOLATED_PREFS === '1' ? 'settings.e2e.json' : 'settings.json';

const inTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

let storePromise: Promise<Store> | null = null;
function getStore(): Promise<Store> {
	if (!storePromise) {
		storePromise = (async () => {
			const base = await dataDir();
			const path = await join(base, APP_DATA_FOLDER, STORE_FILE);
			return load(path, { autoSave: true, defaults: {} });
		})();
	}
	return storePromise;
}

export async function getPreference<T>(key: string, defaultValue: T): Promise<T> {
	if (inTauri) {
		try {
			const store = await getStore();
			const value = await store.get<T>(key);
			return value === undefined || value === null ? defaultValue : value;
		} catch {
			// fall through to localStorage
		}
	}
	if (typeof localStorage !== 'undefined') {
		const raw = localStorage.getItem(key);
		if (raw === null) return defaultValue;
		try {
			return JSON.parse(raw) as T;
		} catch {
			return defaultValue;
		}
	}
	return defaultValue;
}

export async function setPreference<T>(key: string, value: T): Promise<void> {
	if (inTauri) {
		try {
			const store = await getStore();
			await store.set(key, value);
			return;
		} catch {
			// fall through to localStorage
		}
	}
	if (typeof localStorage !== 'undefined') {
		localStorage.setItem(key, JSON.stringify(value));
	}
}
