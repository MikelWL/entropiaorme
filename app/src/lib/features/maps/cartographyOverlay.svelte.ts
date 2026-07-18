import { emit } from '@tauri-apps/api/event';
import type { CoordScanResult, MapPinInput } from '$lib/api';
import { getPreference, setPreference } from '$lib/preferences';
import { PIN_ICONS, pinKind } from './pinIcons';

export type CartographyButton = {
	id: string;
	name: string;
	icon: string;
	kind: string;
	radiusM: number | null;
};

export type CartographyOverlayConfig = {
	planet: string | null;
	buttons: CartographyButton[];
};

const PREFERENCE_KEY = 'cartographyOverlay';
export const CARTOGRAPHY_OVERLAY_CHANGED_EVENT = 'cartography-overlay-changed';
export const MAP_PINS_CHANGED_EVENT = 'map-pins-changed';
export const MAX_CARTOGRAPHY_BUTTONS = 8;

export const DEFAULT_CARTOGRAPHY_BUTTONS: CartographyButton[] = [
	{ id: 'ore-claim', name: 'Ore claim', icon: 'ore', kind: 'mining', radiusM: null },
	{ id: 'mob-spawn', name: 'Mob spawn', icon: 'enemy', kind: 'mob', radiusM: 50 },
	{ id: 'favourite', name: 'Favourite', icon: 'star', kind: 'marker', radiusM: null },
];

const DEFAULT_CONFIG: CartographyOverlayConfig = {
	planet: null,
	buttons: DEFAULT_CARTOGRAPHY_BUTTONS,
};

let config = $state<CartographyOverlayConfig>(structuredClone(DEFAULT_CONFIG));

export const cartographyOverlayConfig = {
	get current(): CartographyOverlayConfig {
		return config;
	},
};

function cleanText(value: unknown, fallback: string, max: number): string {
	if (typeof value !== 'string') return fallback;
	return value.trim().slice(0, max) || fallback;
}

export function sanitiseCartographyOverlayConfig(value: unknown): CartographyOverlayConfig {
	if (!value || typeof value !== 'object') return structuredClone(DEFAULT_CONFIG);
	const candidate = value as Partial<CartographyOverlayConfig>;
	const iconIds = new Set(PIN_ICONS.map((icon) => icon.id));
	const seen = new Set<string>();
	const buttons: CartographyButton[] = [];
	if (Array.isArray(candidate.buttons)) {
		for (const raw of candidate.buttons) {
			if (!raw || typeof raw !== 'object' || buttons.length >= MAX_CARTOGRAPHY_BUTTONS) continue;
			const item = raw as Partial<CartographyButton>;
			const id = cleanText(item.id, `button-${buttons.length + 1}`, 64);
			if (seen.has(id)) continue;
			seen.add(id);
			const radius = item.radiusM;
			const icon = typeof item.icon === 'string' && iconIds.has(item.icon) ? item.icon : 'pin';
			buttons.push({
				id,
				name: cleanText(item.name, 'Pin', 40),
				icon,
				kind: pinKind(icon),
				radiusM:
					typeof radius === 'number' && Number.isFinite(radius) && radius > 0
						? Math.min(radius, 10_000)
						: null,
			});
		}
	}
	return {
		planet:
			typeof candidate.planet === 'string' && candidate.planet.trim()
				? candidate.planet.trim().slice(0, 80)
				: null,
		buttons: buttons.length ? buttons : structuredClone(DEFAULT_CARTOGRAPHY_BUTTONS),
	};
}

export async function initCartographyOverlay(): Promise<void> {
	config = sanitiseCartographyOverlayConfig(
		await getPreference<unknown>(PREFERENCE_KEY, DEFAULT_CONFIG),
	);
}

export async function setCartographyOverlayConfig(value: CartographyOverlayConfig): Promise<void> {
	const clean = sanitiseCartographyOverlayConfig(value);
	config = clean;
	await setPreference(PREFERENCE_KEY, clean);
	void emit(CARTOGRAPHY_OVERLAY_CHANGED_EVENT, clean);
}

export function acceptCartographyOverlayBroadcast(value: unknown): void {
	config = sanitiseCartographyOverlayConfig(value);
}

export function cartographyScanFailureMessage(
	status: CoordScanResult['status'],
	planet: string,
): string {
	const messages: Partial<Record<CoordScanResult['status'], string>> = {
		noRegion: 'Calibrate capture in Maps first.',
		captureFailed: 'Screen capture failed.',
		engineUnavailable: 'Text recognition is unavailable.',
		unreadable: 'Coordinates could not be read.',
		implausible: `The readout is outside ${planet}.`,
	};
	return messages[status] ?? 'Coordinate scan failed.';
}

export function cartographyPinInput(
	planet: string,
	button: CartographyButton,
	result: CoordScanResult,
): MapPinInput | null {
	if (result.status !== 'read' || result.lon == null || result.lat == null) return null;
	return {
		planet,
		lon: result.lon,
		lat: result.lat,
		altitude: result.altitude,
		name: button.name,
		icon: button.icon,
		kind: button.kind,
		radiusM: button.radiusM,
		notes: null,
		sessionId: null,
	};
}
