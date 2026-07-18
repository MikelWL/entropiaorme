import { beforeEach, describe, expect, it, vi } from 'vitest';

const getPreference = vi.fn();
const setPreference = vi.fn();
const emit = vi.fn();

vi.mock('$lib/preferences', () => ({
	getPreference: (...args: unknown[]) => getPreference(...args),
	setPreference: (...args: unknown[]) => setPreference(...args),
}));
vi.mock('@tauri-apps/api/event', () => ({
	emit: (...args: unknown[]) => emit(...args),
}));

type Mod = typeof import('./cartographyOverlay.svelte');

async function loadModule(): Promise<Mod> {
	vi.resetModules();
	return import('./cartographyOverlay.svelte');
}

beforeEach(() => {
	getPreference.mockReset();
	setPreference.mockReset().mockResolvedValue(undefined);
	emit.mockReset().mockResolvedValue(undefined);
});

describe('cartography overlay preferences', () => {
	it('recovers corrupt data to the useful default palette', async () => {
		const { sanitiseCartographyOverlayConfig, DEFAULT_CARTOGRAPHY_BUTTONS } = await loadModule();
		expect(sanitiseCartographyOverlayConfig(null)).toEqual({
			planet: null,
			buttons: DEFAULT_CARTOGRAPHY_BUTTONS,
		});
	});

	it('sanitises and bounds persisted button definitions', async () => {
		const { sanitiseCartographyOverlayConfig, MAX_CARTOGRAPHY_BUTTONS } = await loadModule();
		const buttons = Array.from({ length: 12 }, (_, index) => ({
			id: `id-${index}`,
			name: index === 0 ? '   ' : `Button ${index}`,
			icon: index === 1 ? 'unknown' : 'ore',
			kind: '',
			radiusM: index === 2 ? -5 : 20_000,
		}));
		const clean = sanitiseCartographyOverlayConfig({ planet: ' Calypso ', buttons });
		expect(clean.planet).toBe('Calypso');
		expect(clean.buttons).toHaveLength(MAX_CARTOGRAPHY_BUTTONS);
		expect(clean.buttons[0]).toMatchObject({ name: 'Pin', kind: 'mining', radiusM: 10_000 });
		expect(clean.buttons[1].icon).toBe('pin');
		expect(clean.buttons[2].radiusM).toBeNull();
	});

	it('persists a clean config and broadcasts it to every window', async () => {
		const {
			setCartographyOverlayConfig,
			cartographyOverlayConfig,
			CARTOGRAPHY_OVERLAY_CHANGED_EVENT,
		} = await loadModule();
		await setCartographyOverlayConfig({
			planet: 'Calypso',
			buttons: [{ id: 'one', name: ' North ', icon: 'star', kind: '', radiusM: 50 }],
		});
		const expected = {
			planet: 'Calypso',
			buttons: [{ id: 'one', name: 'North', icon: 'star', kind: 'favourite', radiusM: 50 }],
		};
		expect(cartographyOverlayConfig.current).toEqual(expected);
		expect(setPreference).toHaveBeenCalledWith('cartographyOverlay', expected);
		expect(emit).toHaveBeenCalledWith(CARTOGRAPHY_OVERLAY_CHANGED_EVENT, expected);
	});

	it('loads and sanitises the stored preference', async () => {
		getPreference.mockResolvedValue({
			planet: 'Calypso',
			buttons: [{ id: 'one', name: 'Spot', icon: 'bogus', kind: 'marker', radiusM: null }],
		});
		const { initCartographyOverlay, cartographyOverlayConfig } = await loadModule();
		await initCartographyOverlay();
		expect(getPreference).toHaveBeenCalledWith('cartographyOverlay', expect.any(Object));
		expect(cartographyOverlayConfig.current.buttons[0].icon).toBe('pin');
	});

	it('maps a successful scan and configured button to the exact typed pin input', async () => {
		const { cartographyPinInput } = await loadModule();
		expect(
			cartographyPinInput(
				'Calypso',
				{ id: 'ore', name: 'Claim', icon: 'ore', kind: 'mining', radiusM: 50 },
				{ status: 'read', lon: 61_234, lat: 75_456, altitude: 103, rawText: '61234 75456 103' },
			),
		).toEqual({
			planet: 'Calypso',
			lon: 61_234,
			lat: 75_456,
			altitude: 103,
			name: 'Claim',
			icon: 'ore',
			kind: 'mining',
			radiusM: 50,
			notes: null,
			sessionId: null,
		});
	});

	it('refuses a read result without both coordinates', async () => {
		const { cartographyPinInput } = await loadModule();
		expect(
			cartographyPinInput(
				'Calypso',
				{ id: 'pin', name: 'Pin', icon: 'pin', kind: 'marker', radiusM: null },
				{ status: 'read', lon: null, lat: 75_456, altitude: null, rawText: null },
			),
		).toBeNull();
	});
});
