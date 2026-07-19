/**
 * Colour for the fine-grained precision marker rendering (the low-opacity
 * points and density cells that stand in for emoji markers at scale). The
 * user picks a hex colour; the additive-opacity compositing that keeps
 * overlapping trees legible is preserved by only ever varying the alpha.
 */

export const DEFAULT_PRECISION_MARKER_COLOUR = '#38bdf8';

/** Preference key for the persisted precision-marker colour. */
export const PRECISION_MARKER_COLOUR_KEY = 'precisionMarkerColour';

export type Rgb = { r: number; g: number; b: number };

const DEFAULT_RGB: Rgb = { r: 56, g: 189, b: 248 };

/**
 * Parse a `#rrggbb` (or `rrggbb`) hex colour. Falls back to the default
 * sky-blue for anything unparseable so a corrupt preference never blanks
 * the markers.
 */
export function parseHexRgb(hex: string): Rgb {
	const match = /^#?([0-9a-f]{6})$/i.exec(hex.trim());
	if (!match) return { ...DEFAULT_RGB };
	const value = Number.parseInt(match[1], 16);
	return { r: (value >> 16) & 0xff, g: (value >> 8) & 0xff, b: value & 0xff };
}

/** Normalise arbitrary input to a canonical lowercase `#rrggbb` string. */
export function normaliseHex(hex: string): string {
	const { r, g, b } = parseHexRgb(hex);
	return `#${[r, g, b].map((c) => c.toString(16).padStart(2, '0')).join('')}`;
}

/** Build an `rgba(...)` fill string from the chosen colour and an alpha. */
export function markerRgba(hex: string, alpha: number): string {
	const { r, g, b } = parseHexRgb(hex);
	const a = Math.min(1, Math.max(0, alpha));
	return `rgba(${r}, ${g}, ${b}, ${a})`;
}
