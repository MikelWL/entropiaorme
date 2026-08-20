// @vitest-environment happy-dom

import { describe, expect, it } from 'vitest';
import type { ActivityOption, ActivityOptionsResult } from '$lib/api';
import {
	buildActivitiesMenuState,
	buildQuestHandInMenuState,
	computeMenuHeight,
	menuRowCount,
} from './overlayMenu';

function option(overrides: Partial<ActivityOption> = {}): ActivityOption {
	return {
		key: 'quest:11',
		kind: 'quest',
		name: 'Daily: Carabok',
		questId: 11,
		active: false,
		available: true,
		unavailableReason: null,
		availableFrom: null,
		offRoster: false,
		manualHandIn: false,
		handInWaiting: false,
		...overrides,
	};
}

function offerings(overrides: Partial<ActivityOptionsResult> = {}): ActivityOptionsResult {
	return {
		visible: true,
		adHocSegments: false,
		readyCount: 1,
		options: [option()],
		active: [],
		...overrides,
	};
}

describe('the Activities menu state', () => {
	it('carries the typed name, so a re-presented menu can hand it back', () => {
		// The panel's input is a view of the model's buffer. Without this
		// field a refused declaration cleared the input and stranded the
		// name where nothing rendered it.
		const state = buildActivitiesMenuState(
			180,
			offerings({ adHocSegments: true }),
			false,
			'Boss lap',
		);

		expect(state.segmentDraft).toBe('Boss lap');
	});

	it('carries an empty draft as readily, which is how a landed declaration clears the field', () => {
		const state = buildActivitiesMenuState(180, offerings({ adHocSegments: true }), false, '');

		expect(state.segmentDraft).toBe('');
	});

	it('reports whether a session is running, which is what disables every row', () => {
		expect(buildActivitiesMenuState(180, offerings(), true, '').idle).toBe(true);
		expect(buildActivitiesMenuState(180, offerings(), false, '').idle).toBe(false);
	});

	it('counts the naming field as a row, so the window is sized to hold it', () => {
		const rows = menuRowCount(buildActivitiesMenuState(180, offerings(), false, ''));
		const withEntry = menuRowCount(
			buildActivitiesMenuState(180, offerings({ adHocSegments: true }), false, ''),
		);

		expect(withEntry).toBe(rows + 1);
	});

	it('never sizes to nothing, so the empty state has a line to render on', () => {
		const state = buildActivitiesMenuState(180, offerings({ options: [] }), false, '');

		expect(menuRowCount(state)).toBe(1);
	});
});

describe('the manual hand-in satellite', () => {
	it('reserves its maximum height while waiting so the next candidate cannot be clipped', () => {
		const state = buildQuestHandInMenuState(180, {
			questId: 7,
			questName: 'AI Daily terminal',
			waiting: true,
			candidate: null,
		});

		expect(computeMenuHeight(menuRowCount(state))).toBe(220);
	});
});
