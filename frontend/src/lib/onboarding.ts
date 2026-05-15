import { getPreference, setPreference } from './preferences';

const KEY = 'has_completed_onboarding';

export const getOnboardingComplete = (): Promise<boolean> => getPreference(KEY, false);
export const setOnboardingComplete = (value: boolean): Promise<void> => setPreference(KEY, value);
