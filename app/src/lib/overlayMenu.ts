import type { ManualMobSuggestion } from '$lib/api';

export const OVERLAY_MENU_WINDOW_LABEL = 'overlay-menu';
export const OVERLAY_MENU_READY_EVENT = 'overlay-menu:ready';
export const OVERLAY_MENU_SHOW_EVENT = 'overlay-menu:show';
export const OVERLAY_MENU_HIDE_EVENT = 'overlay-menu:hide';
export const OVERLAY_MENU_SELECT_EVENT = 'overlay-menu:select';
export const OVERLAY_MENU_CLOSED_EVENT = 'overlay-menu:closed';
export const OVERLAY_MENU_INTERACT_EVENT = 'overlay-menu:interact';

export type OverlayMenuKind = 'mob' | 'trifecta';

export type OverlayMenuState = OverlayTrifectaMenuState | OverlayMobMenuState;

export interface OverlayTrifectaMenuState {
	kind: 'trifecta';
	width: number;
	options: {
		id: string;
		name: string;
		active: boolean;
	}[];
}

export interface OverlayMobMenuState {
	kind: 'mob';
	width: number;
	mode: 'tag' | 'manual';
	query: string;
	loading: boolean;
	error: string | null;
	tagSuggestions: string[];
	mobSuggestions: ManualMobSuggestion[];
}

export type OverlayMenuSelection =
	| { kind: 'trifecta'; presetId: string }
	| { kind: 'tag'; tag: string }
	| { kind: 'mob'; species: string; maturity: string };
