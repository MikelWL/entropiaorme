import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { SessionDetail, TrackingSession } from '$lib/types/tracking';
import { createSessionsModel, PAGE_SIZE } from './sessionsModel.svelte';

vi.mock('$lib/api', () => ({
	getTrackingSessions: vi.fn(),
	getSessionDetail: vi.fn(),
	deleteSession: vi.fn(),
}));

import * as api from '$lib/api';

const mocked = vi.mocked(api);

function session(overrides: Partial<TrackingSession> = {}): TrackingSession {
	return {
		id: 's1',
		startTime: '2026-07-01T10:00:00Z',
		duration: 3600,
		primaryMobs: ['Atrox Young'],
		net: 1.5,
		globals: 0,
		hofs: 0,
		...overrides,
	} as TrackingSession;
}

function detail(): SessionDetail {
	return { id: 's1' } as unknown as SessionDetail;
}

function page(
	sessions: TrackingSession[],
	nextCursor: string | null = null,
	total: number = sessions.length,
) {
	return { sessions, nextCursor, total };
}

beforeEach(() => {
	vi.clearAllMocks();
});

describe('loadSessions', () => {
	it('loads the session list and clears the loading flag', async () => {
		mocked.getTrackingSessions.mockResolvedValue(page([session()]));
		const model = createSessionsModel();
		await model.loadSessions();

		expect(model.sessions).toHaveLength(1);
		expect(model.loading).toBe(false);
		expect(model.error).toBeNull();
	});

	it('surfaces a load failure and clears a stale error on entry', async () => {
		mocked.getTrackingSessions.mockRejectedValueOnce(new Error('backend unreachable'));
		const model = createSessionsModel();
		await model.loadSessions();
		expect(model.error).toBe('backend unreachable');

		mocked.getTrackingSessions.mockResolvedValue(page([]));
		await model.loadSessions();
		expect(model.error).toBeNull();
	});
});

describe('paging', () => {
	it('pages ten sessions per page in backend order', async () => {
		mocked.getTrackingSessions.mockResolvedValue(
			page(Array.from({ length: 23 }, (_, i) => session({ id: `s${i}` }))),
		);
		const model = createSessionsModel();
		await model.loadSessions();

		expect(PAGE_SIZE).toBe(10);
		expect(model.table.totalPages).toBe(3);
		expect(model.table.pageRows.map((s) => s.id)).toEqual(
			Array.from({ length: 10 }, (_, i) => `s${i}`),
		);

		model.table.page = 2;
		expect(model.table.pageRows.map((s) => s.id)).toEqual(['s20', 's21', 's22']);
	});

	it('clamps the page when the list shrinks past the last page', async () => {
		mocked.getTrackingSessions.mockResolvedValue(
			page(Array.from({ length: 11 }, (_, i) => session({ id: `s${i}` }))),
		);
		mocked.deleteSession.mockResolvedValue(undefined);
		const model = createSessionsModel();
		await model.loadSessions();

		model.table.page = 1;
		await model.handleDelete('s10');
		expect(model.table.totalPages).toBe(1);
		expect(model.table.page).toBe(0);
	});
});

describe('loadMoreSessions', () => {
	it('appends the next keyset page and grows the pager range', async () => {
		mocked.getTrackingSessions.mockResolvedValueOnce(
			page([session({ id: 's1' }), session({ id: 's2' })], 'cursor-1'),
		);
		const model = createSessionsModel();
		await model.loadSessions();
		expect(model.nextCursor).toBe('cursor-1');

		mocked.getTrackingSessions.mockResolvedValueOnce(page([session({ id: 's3' })], null));
		await model.loadMoreSessions();

		expect(mocked.getTrackingSessions).toHaveBeenLastCalledWith('cursor-1');
		expect(model.sessions.map((s) => s.id)).toEqual(['s1', 's2', 's3']);
		expect(model.nextCursor).toBeNull();
	});

	it('is a no-op with no cursor and while a fetch is in flight', async () => {
		mocked.getTrackingSessions.mockResolvedValue(page([session()]));
		const model = createSessionsModel();
		await model.loadSessions();

		await model.loadMoreSessions();
		expect(mocked.getTrackingSessions).toHaveBeenCalledTimes(1);
	});

	it('surfaces a load-more failure and keeps the loaded window', async () => {
		mocked.getTrackingSessions.mockResolvedValueOnce(page([session()], 'cursor-1'));
		const model = createSessionsModel();
		await model.loadSessions();

		mocked.getTrackingSessions.mockRejectedValueOnce(new Error('backend unreachable'));
		await model.loadMoreSessions();
		expect(model.error).toBe('backend unreachable');
		expect(model.sessions).toHaveLength(1);
		expect(model.nextCursor).toBe('cursor-1');
	});
});

describe('on-demand paging', () => {
	it('reports totals from the server, not the loaded window', async () => {
		mocked.getTrackingSessions.mockResolvedValueOnce(
			page(
				Array.from({ length: 10 }, (_, i) => session({ id: `s${i}` })),
				'cursor-1',
				43,
			),
		);
		const model = createSessionsModel();
		await model.loadSessions();

		expect(model.total).toBe(43);
		expect(model.totalPages).toBe(5);
	});

	it('fetches the next keyset page when Next steps past the loaded window', async () => {
		mocked.getTrackingSessions.mockResolvedValueOnce(
			page(
				Array.from({ length: 10 }, (_, i) => session({ id: `s${i}` })),
				'cursor-1',
				15,
			),
		);
		const model = createSessionsModel();
		await model.loadSessions();

		mocked.getTrackingSessions.mockResolvedValueOnce(
			page(
				Array.from({ length: 5 }, (_, i) => session({ id: `s${10 + i}` })),
				null,
				15,
			),
		);
		await model.nextPage();

		expect(mocked.getTrackingSessions).toHaveBeenLastCalledWith('cursor-1');
		expect(model.table.page).toBe(1);
		expect(model.table.pageRows.map((s) => s.id)).toEqual(
			Array.from({ length: 5 }, (_, i) => `s${10 + i}`),
		);
	});

	it('steps within the loaded window without a fetch', async () => {
		mocked.getTrackingSessions.mockResolvedValueOnce(
			page(
				Array.from({ length: 20 }, (_, i) => session({ id: `s${i}` })),
				'cursor-1',
				30,
			),
		);
		const model = createSessionsModel();
		await model.loadSessions();

		await model.nextPage();
		expect(mocked.getTrackingSessions).toHaveBeenCalledTimes(1);
		expect(model.table.page).toBe(1);

		model.prevPage();
		expect(model.table.page).toBe(0);
	});

	it('holds the page when the on-demand fetch fails, and stops at the true last page', async () => {
		mocked.getTrackingSessions.mockResolvedValueOnce(
			page(
				Array.from({ length: 10 }, (_, i) => session({ id: `s${i}` })),
				'cursor-1',
				15,
			),
		);
		const model = createSessionsModel();
		await model.loadSessions();

		mocked.getTrackingSessions.mockRejectedValueOnce(new Error('backend unreachable'));
		await model.nextPage();
		expect(model.table.page).toBe(0);
		expect(model.error).toBe('backend unreachable');

		mocked.getTrackingSessions.mockResolvedValueOnce(
			page(
				Array.from({ length: 5 }, (_, i) => session({ id: `s${10 + i}` })),
				null,
				15,
			),
		);
		await model.nextPage();
		expect(model.table.page).toBe(1);
		await model.nextPage();
		expect(model.table.page).toBe(1);
	});
});

describe('toggleSession', () => {
	it('expands a row with its detail and collapses on the second toggle', async () => {
		mocked.getTrackingSessions.mockResolvedValue(page([session()]));
		mocked.getSessionDetail.mockResolvedValue(detail());
		const model = createSessionsModel();
		await model.loadSessions();

		await model.toggleSession('s1');
		expect(model.expandedSessionId).toBe('s1');
		expect(model.expandedDetail).not.toBeNull();
		expect(model.loadingDetail).toBe(false);

		await model.toggleSession('s1');
		expect(model.expandedSessionId).toBeNull();
		expect(model.expandedDetail).toBeNull();
	});

	it('keeps the row expanded with no detail when the detail fetch fails', async () => {
		mocked.getTrackingSessions.mockResolvedValue(page([session()]));
		mocked.getSessionDetail.mockRejectedValue(new Error('gone'));
		const model = createSessionsModel();
		await model.loadSessions();

		await model.toggleSession('s1');
		expect(model.expandedSessionId).toBe('s1');
		expect(model.expandedDetail).toBeNull();
	});
});

describe('handleDelete', () => {
	it('removes the session, collapses it if expanded, and resets the confirm state', async () => {
		mocked.getTrackingSessions.mockResolvedValue(page([session(), session({ id: 's2' })]));
		mocked.getSessionDetail.mockResolvedValue(detail());
		mocked.deleteSession.mockResolvedValue(undefined);
		const model = createSessionsModel();
		await model.loadSessions();
		await model.toggleSession('s1');
		model.confirmDeleteId = 's1';

		await model.handleDelete('s1');
		expect(model.sessions.map((s) => s.id)).toEqual(['s2']);
		expect(model.expandedSessionId).toBeNull();
		expect(model.confirmDeleteId).toBeNull();
	});

	it('ignores a re-entrant delete while one is in flight', async () => {
		mocked.getTrackingSessions.mockResolvedValue(page([session()]));
		let resolveDelete!: () => void;
		mocked.deleteSession.mockImplementation(
			() =>
				new Promise<void>((res) => {
					resolveDelete = res;
				}),
		);
		const model = createSessionsModel();
		await model.loadSessions();

		const first = model.handleDelete('s1');
		await model.handleDelete('s1');
		expect(mocked.deleteSession).toHaveBeenCalledTimes(1);
		resolveDelete();
		await first;
		expect(model.sessions).toHaveLength(0);
	});

	it('surfaces a delete failure and keeps the row', async () => {
		mocked.getTrackingSessions.mockResolvedValue(page([session()]));
		mocked.deleteSession.mockRejectedValue(new Error('delete failed'));
		const model = createSessionsModel();
		await model.loadSessions();

		await model.handleDelete('s1');
		expect(model.error).toBe('delete failed');
		expect(model.sessions).toHaveLength(1);
	});
});

describe('guide row control', () => {
	it('expands the row at a page-local index and collapses all', async () => {
		mocked.getTrackingSessions.mockResolvedValue(page([session(), session({ id: 's2' })]));
		mocked.getSessionDetail.mockResolvedValue(detail());
		const model = createSessionsModel();
		await model.loadSessions();

		model.expandAtIndex(1);
		await vi.waitFor(() => expect(model.expandedSessionId).toBe('s2'));

		model.collapseAll();
		expect(model.expandedSessionId).toBeNull();

		model.expandAtIndex(99);
		expect(model.expandedSessionId).toBeNull();
	});
});
