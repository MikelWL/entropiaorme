import { emit } from '@tauri-apps/api/event';
import type { MapPin, PlanetMapCalibration } from '$lib/api';
import { gameToImage } from './coords';

export const ROUTE_AREA_SELECTION_REQUEST_EVENT = 'navigation-area-selection-requested';
export const ROUTE_AREA_SELECTION_RESULT_EVENT = 'navigation-area-selection-result';
export const ROUTE_AREA_SELECTION_CANCELLED_EVENT = 'navigation-area-selection-cancelled';
export const ROUTE_AREA_SELECTION_RESET_EVENT = 'navigation-area-selection-reset';

export type ImagePoint = { x: number; y: number };
export type ImageRect = { left: number; top: number; right: number; bottom: number };
export type RouteAreaSelectionRequest = {
	requestId: number;
	planet: string;
	mapViewId: number | null;
};
export type RouteAreaSelectionResult = RouteAreaSelectionRequest & { pinIds: number[] };

export function acceptRouteAreaSelectionCancellation(value: unknown): number | null {
	const requestId = (value as { requestId?: unknown } | null)?.requestId;
	return typeof requestId === 'number' && Number.isSafeInteger(requestId) && requestId > 0
		? requestId
		: null;
}

export function acceptRouteAreaSelectionRequest(value: unknown): RouteAreaSelectionRequest | null {
	const candidate = (value ?? {}) as Partial<RouteAreaSelectionRequest>;
	if (
		typeof candidate.requestId !== 'number' ||
		!Number.isSafeInteger(candidate.requestId) ||
		candidate.requestId <= 0 ||
		typeof candidate.planet !== 'string' ||
		!candidate.planet.trim()
	) {
		return null;
	}
	const mapViewId = candidate.mapViewId;
	if (
		mapViewId !== null &&
		(typeof mapViewId !== 'number' || !Number.isSafeInteger(mapViewId) || mapViewId <= 0)
	) {
		return null;
	}
	return {
		requestId: candidate.requestId,
		planet: candidate.planet.trim().slice(0, 80),
		mapViewId,
	};
}

export function acceptRouteAreaSelectionResult(value: unknown): RouteAreaSelectionResult | null {
	const request = acceptRouteAreaSelectionRequest(value);
	const candidate = (value ?? {}) as Partial<RouteAreaSelectionResult>;
	if (!request || !Array.isArray(candidate.pinIds)) return null;
	const pinIds = candidate.pinIds.filter(
		(id): id is number => typeof id === 'number' && Number.isSafeInteger(id) && id > 0,
	);
	if (pinIds.length === 0 || pinIds.length !== candidate.pinIds.length) return null;
	return { ...request, pinIds: [...new Set(pinIds)].sort((left, right) => left - right) };
}

export function normaliseImageRect(start: ImagePoint, end: ImagePoint): ImageRect {
	return {
		left: Math.min(start.x, end.x),
		top: Math.min(start.y, end.y),
		right: Math.max(start.x, end.x),
		bottom: Math.max(start.y, end.y),
	};
}

export function clampImageRect(rect: ImageRect, width: number, height: number): ImageRect {
	return {
		left: Math.max(0, Math.min(width, rect.left)),
		top: Math.max(0, Math.min(height, rect.top)),
		right: Math.max(0, Math.min(width, rect.right)),
		bottom: Math.max(0, Math.min(height, rect.bottom)),
	};
}

export function imageRectContains(rect: ImageRect, point: ImagePoint): boolean {
	return (
		point.x >= rect.left && point.x <= rect.right && point.y >= rect.top && point.y <= rect.bottom
	);
}

export function isEligibleRoutePin(pin: MapPin, nowEpoch: number): boolean {
	return pin.specialKind === 'tree' && (pin.cooldownUntil == null || pin.cooldownUntil <= nowEpoch);
}

export function selectedMapPinIds(
	pins: MapPin[],
	calibration: PlanetMapCalibration,
	regions: ImageRect[],
): number[] {
	return pins
		.filter((pin) => {
			const point = gameToImage(calibration, { lon: pin.lon, lat: pin.lat });
			return regions.some((region) => imageRectContains(region, point));
		})
		.map((pin) => pin.id)
		.sort((left, right) => left - right);
}

export function selectedTreePinIds(pins: MapPin[], selectedIds: number[]): number[] {
	const selected = new Set(selectedIds);
	return pins
		.filter((pin) => selected.has(pin.id) && pin.specialKind === 'tree')
		.map((pin) => pin.id)
		.sort((left, right) => left - right);
}

export function selectedRoutePinIds(
	pins: MapPin[],
	calibration: PlanetMapCalibration,
	regions: ImageRect[],
	nowEpoch: number,
): number[] {
	const eligiblePins = pins.filter((pin) => isEligibleRoutePin(pin, nowEpoch));
	return selectedMapPinIds(eligiblePins, calibration, regions);
}

export function emitRouteAreaSelectionResult(result: RouteAreaSelectionResult): void {
	void emit(ROUTE_AREA_SELECTION_RESULT_EVENT, result);
}

export function emitRouteAreaSelectionCancelled(requestId: number): void {
	void emit(ROUTE_AREA_SELECTION_CANCELLED_EVENT, { requestId });
}

export function resetRouteAreaSelection(): void {
	void emit(ROUTE_AREA_SELECTION_RESET_EVENT);
}
