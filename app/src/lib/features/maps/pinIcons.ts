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
	kind: string;
}

export const PIN_ICONS: PinIconDef[] = [
	{ id: 'pin', label: 'Pin', glyph: '📍', kind: 'marker' },
	{ id: 'teleporter', label: 'Teleporter', glyph: '🌀', kind: 'travel' },
	{ id: 'ore', label: 'Ore claim', glyph: '⛏️', kind: 'mining' },
	{ id: 'enemy', label: 'Mob spawn', glyph: '👾', kind: 'hunting' },
	{ id: 'boss', label: 'Boss', glyph: '💀', kind: 'hunting' },
	{ id: 'vendor', label: 'Vendor', glyph: '🏪', kind: 'service' },
	{ id: 'sweat', label: 'Sweat circle', glyph: '💧', kind: 'gathering' },
	{ id: 'home', label: 'Base', glyph: '🏠', kind: 'location' },
	{ id: 'star', label: 'Favourite', glyph: '⭐', kind: 'favourite' },
	{ id: 'flag', label: 'Flag', glyph: '🚩', kind: 'marker' },
];

const FALLBACK = PIN_ICONS[0];

/** The glyph for an icon id, with the fallback for unknown ids. */
export function pinGlyph(iconId: string): string {
	return PIN_ICONS.find((icon) => icon.id === iconId)?.glyph ?? FALLBACK.glyph;
}

/** The stable persisted category implied by a marker choice. */
export function pinKind(iconId: string): string {
	return PIN_ICONS.find((icon) => icon.id === iconId)?.kind ?? FALLBACK.kind;
}
