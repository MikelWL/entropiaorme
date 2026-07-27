// @vitest-environment happy-dom

import { render, screen } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { SessionDetail as SessionDetailType } from '$lib/types/tracking';
import SessionDetail from './SessionDetail.svelte';

vi.mock('$lib/api', () => ({
	ApiError: class ApiError extends Error {},
	activateLootItem: vi.fn(),
	deactivateLootItem: vi.fn(),
	getSessionDetail: vi.fn(),
	renameSession: vi.fn(),
	renameSessionMob: vi.fn(),
	restoreSessionMob: vi.fn(),
}));

import * as api from '$lib/api';

const mocked = vi.mocked(api);

function detail(overrides: Partial<SessionDetailType> = {}): SessionDetailType {
	return {
		sessionId: 's1',
		sessionName: null,
		summary: {
			cost: 10,
			returns: 12,
			pes: 1,
			net: 2,
			returnRate: 1.2,
			kills: 5,
			duration: 600,
			costBreakdown: {
				weaponCost: 10,
				healCost: 0,
				enhancerCost: 0,
				armourCost: 0,
				harvestCost: 0,
			},
		},
		harvest: { swings: 0, successes: 0, lootTt: 0, cost: 0 },
		mobEntryMode: 'mob',
		notableEvents: [],
		lootBreakdown: [],
		deactivatedLootBreakdown: [],
		mobBreakdown: [],
		effectiveLoot: 12,
		toolStats: [],
		skillGains: [],
		...overrides,
	} as SessionDetailType;
}

beforeEach(() => {
	vi.clearAllMocks();
});

// The name is session-grain, so the overlay withholds it once a session
// runs. That is only honest if the record can still correct it, which is
// what this affordance is for.
describe('session name correction', () => {
	it('shows the recorded name, and Unnamed when there is none', () => {
		const { unmount } = render(SessionDetail, {
			props: { detail: detail({ sessionName: 'Ark Monura Instance' }) },
		});
		expect(screen.getByText('Ark Monura Instance')).toBeTruthy();
		unmount();

		render(SessionDetail, { props: { detail: detail() } });
		expect(screen.getByText('Unnamed')).toBeTruthy();
	});

	it('renames through the record and refetches', async () => {
		mocked.renameSession.mockResolvedValue({
			sessionId: 's1',
			sessionName: 'Ark Carabok Instance',
		} as Awaited<ReturnType<typeof api.renameSession>>);
		mocked.getSessionDetail.mockResolvedValue(
			detail({ sessionName: 'Ark Carabok Instance' }) as Awaited<
				ReturnType<typeof api.getSessionDetail>
			>,
		);

		render(SessionDetail, { props: { detail: detail({ sessionName: 'Wrong Name' }) } });
		screen.getByText('Rename').click();

		const input = (await screen.findByLabelText('Session name')) as HTMLInputElement;
		expect(input.value).toBe('Wrong Name');
		input.value = 'Ark Carabok Instance';
		input.dispatchEvent(new Event('input', { bubbles: true }));

		screen.getByText('Save').click();
		await vi.waitFor(() =>
			expect(mocked.renameSession).toHaveBeenCalledWith('s1', 'Ark Carabok Instance'),
		);
	});

	// Clearing the field records "no name", never an empty string that
	// would mint its own bucket on the comparison axis.
	it('sends a cleared name as null', async () => {
		mocked.renameSession.mockResolvedValue({
			sessionId: 's1',
			sessionName: null,
		} as Awaited<ReturnType<typeof api.renameSession>>);
		mocked.getSessionDetail.mockResolvedValue(
			detail() as Awaited<ReturnType<typeof api.getSessionDetail>>,
		);

		render(SessionDetail, { props: { detail: detail({ sessionName: 'Wrong Name' }) } });
		screen.getByText('Rename').click();

		const input = (await screen.findByLabelText('Session name')) as HTMLInputElement;
		input.value = '   ';
		input.dispatchEvent(new Event('input', { bubbles: true }));

		screen.getByText('Save').click();
		await vi.waitFor(() => expect(mocked.renameSession).toHaveBeenCalledWith('s1', null));
	});
});
