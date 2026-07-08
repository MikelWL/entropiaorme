import { getPreference, setPreference } from './preferences';

export type Theme = 'dark' | 'light';
export const DEFAULT_THEME: Theme = 'dark';

const KEY = 'theme';

let current = $state<Theme>(DEFAULT_THEME);

/** The active theme; writes go through `setTheme` so the choice persists. */
export const theme = {
	get current(): Theme {
		return current;
	},
};

export async function initTheme(): Promise<void> {
	const value = await getPreference<Theme>(KEY, DEFAULT_THEME);
	current = value === 'light' ? 'light' : 'dark';
}

export async function setTheme(value: Theme): Promise<void> {
	current = value;
	await setPreference(KEY, value);
}
