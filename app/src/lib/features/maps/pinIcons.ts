/**
 * Legacy pin-icon ids and the Unicode emoji compatibility boundary.
 * New pins persist an emoji directly; older id-based pins continue to
 * render through this map.
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
const EMOJI_COMPONENT = /\p{Extended_Pictographic}|\p{Regional_Indicator}|[#*0-9]\uFE0F?\u20E3/u;

export function normalisePinEmoji(value: unknown): string {
	if (typeof value !== 'string') return FALLBACK.glyph;
	const legacy = PIN_ICONS.find((icon) => icon.id === value);
	if (legacy) return legacy.glyph;
	const emoji = value.trim();
	return emoji.length <= 32 && EMOJI_COMPONENT.test(emoji) ? emoji : FALLBACK.glyph;
}

/** The display glyph for either a current emoji or a legacy icon id. */
export function pinGlyph(icon: string): string {
	return normalisePinEmoji(icon);
}

/** Legacy ids retain their old category; free emoji are neutral markers. */
export function pinKind(icon: string): string {
	return PIN_ICONS.find((definition) => definition.id === icon)?.kind ?? FALLBACK.kind;
}
