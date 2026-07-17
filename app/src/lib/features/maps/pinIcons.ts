/**
 * The pin-icon catalogue: the closed set of glyphs a pin can carry on
 * the map. Stored pins reference an icon by id; an unknown id (an
 * import from a newer version, say) renders the fallback rather than
 * nothing, so a pin never becomes invisible.
 */

export interface PinIconDef {
	id: string;
	label: string;
	glyph: string;
}

export const PIN_ICONS: PinIconDef[] = [
	{ id: 'pin', label: 'Pin', glyph: '📍' },
	{ id: 'teleporter', label: 'Teleporter', glyph: '🌀' },
	{ id: 'ore', label: 'Ore claim', glyph: '⛏️' },
	{ id: 'enemy', label: 'Mob spawn', glyph: '👾' },
	{ id: 'boss', label: 'Boss', glyph: '💀' },
	{ id: 'vendor', label: 'Vendor', glyph: '🏪' },
	{ id: 'sweat', label: 'Sweat circle', glyph: '💧' },
	{ id: 'home', label: 'Base', glyph: '🏠' },
	{ id: 'star', label: 'Favourite', glyph: '⭐' },
	{ id: 'flag', label: 'Flag', glyph: '🚩' },
];

const FALLBACK = PIN_ICONS[0];

/** The glyph for an icon id, with the fallback for unknown ids. */
export function pinGlyph(iconId: string): string {
	return PIN_ICONS.find((icon) => icon.id === iconId)?.glyph ?? FALLBACK.glyph;
}
