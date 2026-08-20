// @vitest-environment happy-dom

import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import type { ActivityOption, ActivityOptionsResult } from '$lib/api';
import type { Quest } from '$lib/types/quests';
import QuestingWidget from './QuestingWidget.svelte';

function option(overrides: Partial<ActivityOption>): ActivityOption {
	return {
		key: 'quest:1',
		kind: 'quest',
		name: 'AI Daily',
		questId: 1,
		active: false,
		available: true,
		unavailableReason: null,
		availableFrom: null,
		offRoster: false,
		manualHandIn: true,
		handInWaiting: false,
		rosterOrder: 0,
		...overrides,
	};
}

function quest(overrides: Partial<Quest> = {}): Quest {
	return {
		id: '1',
		name: 'AI Daily',
		category: null,
		targetMobs: [],
		planet: 'Calypso',
		waypoint: null,
		cooldownDurationHours: null,
		cooldownExpiresAt: null,
		reward: null,
		rewardIsSkill: false,
		rewardDescription: '',
		notes: '',
		chainName: null,
		chainPosition: null,
		chainTotal: null,
		startedAt: null,
		signalLootItem: null,
		completionTrigger: 'manual_hand_in',
		rewardPolicy: 'completion_clump',
		rewardItemNames: [],
		cooldownAnchor: 'completion',
		lastStartedAt: null,
		familyId: null,
		familyName: null,
		familyCooldownDurationHours: null,
		familyCooldownAnchor: null,
		familyCooldownExpiresAt: null,
		...overrides,
	};
}

function props(activityOptions: ActivityOptionsResult | null, quests: Quest[] = [quest()]) {
	return {
		activityOptions,
		quests,
		pendingCancelChoiceQuestId: null,
		copiedWp: null,
		onQuestStart: vi.fn(),
		onQuestComplete: vi.fn(),
		onQuestCancel: vi.fn(),
		onToggleCancelChoice: vi.fn(),
		onCopyWaypoint: vi.fn(),
		onEditSession: vi.fn(),
		getCooldownRemaining: vi.fn(() => null),
	};
}

describe('QuestingWidget', () => {
	it('shows the selected session roster in authored order without segments or off-roster facts', () => {
		const activityOptions: ActivityOptionsResult = {
			definitionId: 7,
			definitionName: 'AI Dailies',
			visible: true,
			adHocSegments: true,
			readyCount: 2,
			options: [
				option({ key: 'quest:2', name: 'Second', questId: 2, rosterOrder: 1 }),
				option({
					key: 'segment:notes',
					kind: 'segment',
					name: 'Notes',
					questId: null,
					rosterOrder: 2,
				}),
				option({ key: 'quest:1', name: 'First', rosterOrder: 0 }),
				option({
					key: 'quest:3',
					name: 'Off roster',
					questId: 3,
					offRoster: true,
					rosterOrder: null,
				}),
			],
			active: [],
		};
		const { container } = render(QuestingWidget, {
			props: props(activityOptions, [quest({ name: 'First' }), quest({ id: '2', name: 'Second' })]),
		});

		expect(screen.getByText('AI Dailies')).not.toBeNull();
		expect(screen.queryByText('Notes')).toBeNull();
		expect(screen.queryByText('Off roster')).toBeNull();
		const rows = Array.from(container.querySelectorAll('.border-b')).filter((row) =>
			row.querySelector('.text-sm.font-medium'),
		);
		expect(
			rows.map((row) => row.querySelector('.text-sm.font-medium')?.textContent).slice(-2),
		).toEqual(['First', 'Second']);
	});

	it('offers one session-authoring action when the selected roster is empty', async () => {
		const activityOptions: ActivityOptionsResult = {
			definitionId: 7,
			definitionName: 'AI Dailies',
			visible: true,
			adHocSegments: false,
			readyCount: 0,
			options: [],
			active: [],
		};
		const model = props(activityOptions, []);
		render(QuestingWidget, { props: model });

		await fireEvent.click(screen.getByRole('button', { name: 'Edit session' }));
		expect(model.onEditSession).toHaveBeenCalledWith(7);
	});

	it('keeps the hand-in waiting state visible in the compact row', () => {
		const activityOptions: ActivityOptionsResult = {
			definitionId: 7,
			definitionName: 'AI Dailies',
			visible: true,
			adHocSegments: false,
			readyCount: 0,
			options: [option({ active: true, available: false, handInWaiting: true })],
			active: [],
		};
		render(QuestingWidget, { props: props(activityOptions, [quest({ startedAt: 100 })]) });

		expect(screen.getByText('Waiting for the next reward clump')).not.toBeNull();
		expect(screen.getByRole('button', { name: 'Hand in' })).not.toBeNull();
	});
});
