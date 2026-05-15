import type { DemoApi, GuideSurface } from './types';

/** Reactive guide-mode state. Single global instance. */
class GuideState {
	isActive = $state(false);
	currentSurface = $state<GuideSurface | null>(null);
	currentStepIndex = $state(0);
	isPlaying = $state(false);
	/**
	 * Virtual-cursor visibility. The cursor is an action indicator: hidden by default at
	 * step boundaries, shown by the `click()` / `hover()` helpers (and by explicit
	 * `cursor.show()` calls) when an active action is being demonstrated. Cards that
	 * just point at a region rely on the cutout highlight alone.
	 */
	cursorVisible = $state(false);
}

export const guideState = new GuideState();

/** Per-surface demoApi registry. Surfaces register on mount, unregister on unmount. */
const demoApiRegistry = new Map<string, DemoApi>();

export function registerDemoApi(surfaceId: string, api: DemoApi): void {
	demoApiRegistry.set(surfaceId, api);
}

export function unregisterDemoApi(surfaceId: string): void {
	demoApiRegistry.delete(surfaceId);
}

export function getDemoApi(surfaceId: string): DemoApi {
	// Returns an empty surface when the guide is inactive, so consumers
	// reaching the registry outside guide mode get a no-op. Registration
	// itself stays onMount to preserve timing across the guide's startup
	// sequence; only the read site is gated.
	if (!guideState.isActive) return {};
	return demoApiRegistry.get(surfaceId) ?? {};
}
