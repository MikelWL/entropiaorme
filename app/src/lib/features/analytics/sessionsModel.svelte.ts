/**
 * Sessions-tab view model: the session list load, row expand/collapse with
 * its detail fetch, deletion, and the client-side pager. Presentation lives
 * in the tab component; it composes over this state.
 *
 * Paging is two-layered by design (the ledger tab's shape): the server
 * side stays keyset (an opaque cursor grows the loaded window on demand
 * as the pager steps past it), while the client-side pager over the
 * loaded window is the shared table model; the server's whole-table
 * count gives the pager its true bounds.
 */

import { deleteSession, getSessionDetail, getTrackingSessions } from '$lib/api';
import type { SessionDetail, TrackingSession } from '$lib/types/tracking';
import { describeError } from '$lib/view/errorState';
import { createTableModel } from '$lib/view/tableModel.svelte';

export const PAGE_SIZE = 10;

export function createSessionsModel() {
	let sessions = $state<TrackingSession[]>([]);
	// The whole-table session count from the server, so the pager reports
	// true bounds rather than the loaded window's size.
	let total = $state(0);
	let loading = $state(true);
	let error = $state<string | null>(null);
	// Keyset pagination: the cursor for the next server page (null once
	// every session is loaded), and whether a "load more" fetch is in flight.
	let nextCursor = $state<string | null>(null);
	let loadingMore = $state(false);
	let expandedSessionId = $state<string | null>(null);
	let expandedDetail = $state<SessionDetail | null>(null);
	let loadingDetail = $state(false);
	let confirmDeleteId = $state<string | null>(null);
	let deleting = $state(false);

	// Pure pager over the loaded window: no search, category, or sort, so
	// the paged rows keep the backend's ordering unchanged.
	const table = createTableModel<TrackingSession>({
		rows: () => sessions,
		pageSize: PAGE_SIZE,
	});

	async function loadSessions() {
		loading = true;
		error = null;
		try {
			const page = await getTrackingSessions();
			sessions = page.sessions;
			nextCursor = page.nextCursor;
			total = page.total;
		} catch (e) {
			error = describeError(e, 'Failed to load sessions');
		} finally {
			loading = false;
		}
	}

	// Fetch the next keyset page and append it, growing the client
	// paginator's range. Older sessions stay reachable without loading the
	// whole history up front.
	async function loadMoreSessions() {
		if (!nextCursor || loadingMore) return;
		error = null;
		loadingMore = true;
		try {
			const page = await getTrackingSessions(nextCursor);
			sessions = [...sessions, ...page.sessions];
			nextCursor = page.nextCursor;
			total = page.total;
		} catch (e) {
			error = describeError(e, 'Failed to load more sessions');
		} finally {
			loadingMore = false;
		}
	}

	// Pager bounds from the server total: the client pages the loaded
	// window, and stepping past it fetches the next keyset page on demand.
	const totalPages = $derived(Math.max(1, Math.ceil(total / PAGE_SIZE)));

	async function nextPage() {
		const nextStart = (table.page + 1) * PAGE_SIZE;
		if (nextStart >= total) return;
		if (nextStart >= sessions.length && nextCursor) await loadMoreSessions();
		if (nextStart < sessions.length) table.page++;
	}

	function prevPage() {
		if (table.page > 0) table.page--;
	}

	async function toggleSession(id: string) {
		if (expandedSessionId === id) {
			expandedSessionId = null;
			expandedDetail = null;
			return;
		}

		expandedSessionId = id;
		expandedDetail = null;
		loadingDetail = true;
		try {
			expandedDetail = await getSessionDetail(id);
		} catch {
			expandedDetail = null;
		} finally {
			loadingDetail = false;
		}
	}

	async function handleDelete(id: string) {
		if (deleting) return;
		error = null;
		deleting = true;
		try {
			await deleteSession(id);
			sessions = sessions.filter((s) => s.id !== id);
			total = Math.max(0, total - 1);
			if (expandedSessionId === id) {
				expandedSessionId = null;
				expandedDetail = null;
			}
		} catch (e) {
			error = describeError(e, 'Failed to delete session');
		}
		deleting = false;
		confirmDeleteId = null;
	}

	function collapseAll() {
		expandedSessionId = null;
		expandedDetail = null;
	}

	function expandAtIndex(idx: number) {
		const target = table.pageRows[idx];
		if (!target) return;
		void toggleSession(target.id);
	}

	return {
		table,

		get sessions() {
			return sessions;
		},
		get loading() {
			return loading;
		},
		get error() {
			return error;
		},
		set error(value: string | null) {
			error = value;
		},
		get nextCursor() {
			return nextCursor;
		},
		get loadingMore() {
			return loadingMore;
		},
		get total() {
			return total;
		},
		get totalPages() {
			return totalPages;
		},
		get expandedSessionId() {
			return expandedSessionId;
		},
		get expandedDetail() {
			return expandedDetail;
		},
		get loadingDetail() {
			return loadingDetail;
		},
		get confirmDeleteId() {
			return confirmDeleteId;
		},
		set confirmDeleteId(value: string | null) {
			confirmDeleteId = value;
		},
		get deleting() {
			return deleting;
		},

		loadSessions,
		loadMoreSessions,
		nextPage,
		prevPage,
		toggleSession,
		handleDelete,
		collapseAll,
		expandAtIndex,
	};
}

export type SessionsModel = ReturnType<typeof createSessionsModel>;
