import type {
	FocusOptionsResult,
	FocusQuestOption,
	ManualMobSuggestion,
	SessionDefinition,
} from '$lib/api';

export const OVERLAY_MENU_WINDOW_LABEL = 'overlay-menu';
export const OVERLAY_MENU_READY_EVENT = 'overlay-menu:ready';
export const OVERLAY_MENU_SHOW_EVENT = 'overlay-menu:show';
export const OVERLAY_MENU_HIDE_EVENT = 'overlay-menu:hide';
export const OVERLAY_MENU_SELECT_EVENT = 'overlay-menu:select';
export const OVERLAY_MENU_CLOSED_EVENT = 'overlay-menu:closed';
export const OVERLAY_MENU_INTERACT_EVENT = 'overlay-menu:interact';

export type OverlayMenuKind = 'definition' | 'mob' | 'trifecta' | 'focus';

export type OverlayMenuState =
	| OverlayTrifectaMenuState
	| OverlayDefinitionMenuState
	| OverlayMobMenuState
	| OverlayFocusMenuState;

export interface OverlayTrifectaMenuState {
	kind: 'trifecta';
	width: number;
	options: {
		id: string;
		name: string;
		active: boolean;
	}[];
}

/** The session picker: the authored definitions with the current
 * selection marked. Tapping the selected row clears the selection;
 * tapping another switches to it. */
export interface OverlayDefinitionMenuState {
	kind: 'definition';
	width: number;
	definitions: {
		id: string;
		name: string;
		selected: boolean;
	}[];
}

/** The declared-mob suggestion menu: catalogue mobs typed ahead. */
export interface OverlayMobMenuState {
	kind: 'mob';
	width: number;
	query: string;
	loading: boolean;
	error: string | null;
	mobSuggestions: ManualMobSuggestion[];
}

/** The focus picker: the in-progress quests (tap to switch focus, tap
 * again to unfocus, `+` to join the standing focus additively) and the
 * segment-name presets recalled for the current session name. */
export interface OverlayFocusMenuState {
	kind: 'focus';
	width: number;
	quests: FocusQuestOption[];
	presets: string[];
}

export type OverlayMenuSelection =
	| { kind: 'trifecta'; presetId: string }
	| { kind: 'definition'; definitionId: string; selected: boolean }
	| { kind: 'mob'; species: string; maturity: string }
	| { kind: 'focus'; action: 'questFocus'; questId: number; additive: boolean }
	| { kind: 'focus'; action: 'questUnfocus'; questId: number }
	| { kind: 'focus'; action: 'preset'; label: string };

/** The widest label's rendered width in the overlay menus' font, for
 * sizing a satellite menu to its content. Falls back to a character
 * estimate where no 2D context is available (test environments). */
export function measureMenuTextWidth(
	labels: string[],
	font = '500 12px Inter, system-ui, sans-serif',
) {
	if (labels.length === 0) return 0;
	const canvas = document.createElement('canvas');
	const context = canvas.getContext('2d');
	if (!context) return labels.reduce((longest, label) => Math.max(longest, label.length * 8), 0);

	context.font = font;
	return labels.reduce((longest, label) => Math.max(longest, context.measureText(label).width), 0);
}

export const OVERLAY_MENU_MIN_WIDTH = 180;
export const OVERLAY_MENU_MAX_WIDTH = 340;
export const OVERLAY_MENU_MAX_HEIGHT = 220;

/** A menu's panel width: the anchor's width as the floor, the widest
 * label plus padding as the content, clamped to the satellite bounds. */
export function computeMenuWidth(minWidth: number, labels: string[], padding: number): number {
	const contentWidth = measureMenuTextWidth(labels);
	return Math.max(
		Math.ceil(minWidth),
		Math.min(
			OVERLAY_MENU_MAX_WIDTH,
			Math.max(OVERLAY_MENU_MIN_WIDTH, Math.ceil(contentWidth + padding)),
		),
	);
}

/** A menu's window height for its row count. */
export function computeMenuHeight(rows: number): number {
	return Math.min(OVERLAY_MENU_MAX_HEIGHT, Math.max(44, rows * 34 + 12));
}

/** Rows a menu state renders, which sets the satellite window's height
 * (the overlay sizes the window from this and the popup route sizes its
 * panel from the same count). Every kind falls back to one row: the
 * loading, error, and empty states each occupy exactly one line. The
 * focus picker counts its section heading as a row when presets follow
 * the quests. */
export function menuRowCount(state: OverlayMenuState): number {
	if (state.kind === 'trifecta') return Math.max(1, state.options.length);
	if (state.kind === 'definition') return Math.max(1, state.definitions.length);
	if (state.kind === 'focus') {
		const headings = state.presets.length > 0 ? 1 : 0;
		return Math.max(1, state.quests.length + state.presets.length + headings);
	}
	if (state.loading || state.error) return 1;
	return Math.max(1, state.mobSuggestions.length);
}

/** The session picker's menu state over the fetched definitions.
 * The width padding leaves room for the Selected badge beside a name. */
export function buildDefinitionMenuState(
	anchorWidth: number,
	definitions: SessionDefinition[],
	selectedId: string | null,
): OverlayDefinitionMenuState {
	const labels =
		definitions.length > 0 ? definitions.map((definition) => definition.name) : ['No sessions yet'];
	return {
		kind: 'definition',
		width: computeMenuWidth(anchorWidth, labels, 96),
		definitions: definitions.map((definition) => ({
			id: definition.id,
			name: definition.name,
			selected: definition.id === selectedId,
		})),
	};
}

/** The focus picker's menu state over the fetched focus options. The
 * extra width padding leaves room for the Focused badge and the
 * additive join button beside a row's name. */
export function buildFocusMenuState(
	anchorWidth: number,
	options: FocusOptionsResult,
): OverlayFocusMenuState {
	const labels = [
		...options.quests.map((quest) => quest.name),
		...options.segmentPresets,
		...(options.segmentPresets.length > 0 ? ['Recent segments'] : []),
	];
	return {
		kind: 'focus',
		width: computeMenuWidth(
			anchorWidth,
			labels.length > 0 ? labels : ['No quests in progress'],
			96,
		),
		quests: options.quests,
		presets: options.segmentPresets,
	};
}
