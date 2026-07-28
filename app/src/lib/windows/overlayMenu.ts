import type { ManualMobSuggestion } from '$lib/api';

export const OVERLAY_MENU_WINDOW_LABEL = 'overlay-menu';
export const OVERLAY_MENU_READY_EVENT = 'overlay-menu:ready';
export const OVERLAY_MENU_SHOW_EVENT = 'overlay-menu:show';
export const OVERLAY_MENU_HIDE_EVENT = 'overlay-menu:hide';
export const OVERLAY_MENU_SELECT_EVENT = 'overlay-menu:select';
export const OVERLAY_MENU_CLOSED_EVENT = 'overlay-menu:closed';
export const OVERLAY_MENU_INTERACT_EVENT = 'overlay-menu:interact';

export type OverlayMenuKind = 'name' | 'mob' | 'quest' | 'trifecta';

export type OverlayMenuState =
	| OverlayTrifectaMenuState
	| OverlayNameMenuState
	| OverlayMobMenuState
	| OverlayQuestMenuState;

export interface OverlayTrifectaMenuState {
	kind: 'trifecta';
	width: number;
	options: {
		id: string;
		name: string;
		active: boolean;
	}[];
}

/** The session-name suggestion menu: prior names typed ahead. */
export interface OverlayNameMenuState {
	kind: 'name';
	width: number;
	query: string;
	loading: boolean;
	error: string | null;
	suggestions: string[];
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

/** The quest-declaration menu: active playlists first, then quests. */
export interface OverlayQuestMenuState {
	kind: 'quest';
	width: number;
	loading: boolean;
	error: string | null;
	options: {
		id: number;
		name: string;
		isPlaylist: boolean;
	}[];
}

export type OverlayMenuSelection =
	| { kind: 'trifecta'; presetId: string }
	| { kind: 'name'; name: string }
	| { kind: 'mob'; species: string; maturity: string }
	| { kind: 'quest'; id: number; isPlaylist: boolean; name: string };

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
