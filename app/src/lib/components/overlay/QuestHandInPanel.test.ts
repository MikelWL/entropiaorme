// @vitest-environment happy-dom

import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { QuestHandInState } from '$lib/api';

const api = vi.hoisted(() => ({
	cancel: vi.fn(),
	confirm: vi.fn(),
	read: vi.fn(),
	waitNext: vi.fn(),
}));

vi.mock('$lib/api', () => ({
	cancelQuestHandIn: api.cancel,
	confirmQuestHandIn: api.confirm,
	getQuestHandInState: api.read,
	waitForNextQuestHandInClump: api.waitNext,
}));

import QuestHandInPanel from './QuestHandInPanel.svelte';

const candidateState: QuestHandInState = {
	questId: 7,
	questName: 'AI Daily terminal',
	waiting: false,
	candidate: {
		id: 42,
		observedAt: '2026-08-20T14:07:26Z',
		totalPed: 31.6423,
		items: [
			{ itemName: 'Universal Ammo', quantity: 316_468, valuePed: 31.64 },
			{ itemName: 'Blazar Fragment', quantity: 238, valuePed: 0.0023 },
		],
	},
};

beforeEach(() => {
	vi.clearAllMocks();
});

describe('QuestHandInPanel', () => {
	it('shows the exact candidate and confirms that identity', async () => {
		const onComplete = vi.fn();
		render(QuestHandInPanel, {
			props: { initialState: candidateState, onComplete, onCancel: vi.fn() },
		});

		expect(screen.getByText('Is this your quest reward?')).toBeTruthy();
		expect(screen.getByText('Universal Ammo')).toBeTruthy();
		expect(screen.getByText(/x316,468/)).toBeTruthy();
		expect(screen.getByText('Blazar Fragment')).toBeTruthy();

		await fireEvent.click(screen.getByRole('button', { name: 'Confirm reward' }));
		await waitFor(() => expect(api.confirm).toHaveBeenCalledWith(7, 42));
		expect(onComplete).toHaveBeenCalledTimes(1);
	});

	it('rejects the candidate into the same next-clump waiting flow', async () => {
		const waitingState = {
			questId: 7,
			questName: 'AI Daily terminal',
			waiting: true,
			candidate: null,
		} satisfies QuestHandInState;
		api.waitNext.mockResolvedValue(waitingState);
		api.read.mockResolvedValue(waitingState);
		render(QuestHandInPanel, {
			props: { initialState: candidateState, onComplete: vi.fn(), onCancel: vi.fn() },
		});

		await fireEvent.click(screen.getByRole('button', { name: 'No, wait for the next clump' }));
		await waitFor(() => expect(api.waitNext).toHaveBeenCalledWith(7, 42));
		expect(screen.getByText('Hand in the quest now')).toBeTruthy();
	});

	it('cancels only the capture flow', async () => {
		const onCancel = vi.fn();
		render(QuestHandInPanel, {
			props: { initialState: candidateState, onComplete: vi.fn(), onCancel },
		});

		await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
		await waitFor(() => expect(api.cancel).toHaveBeenCalledWith(7));
		expect(onCancel).toHaveBeenCalledTimes(1);
	});
});
