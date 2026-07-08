/**
 * Sessions-tab view model: the session list load, row expand/collapse with
 * its detail fetch, deletion, and the client-side pager. Presentation lives
 * in the tab component; it composes over this state.
 */

import { deleteSession, getSessionDetail, getTrackingSessions } from '$lib/api';
import type { SessionDetail, TrackingSession } from '$lib/types/tracking';
import { describeError } from '$lib/view/errorState';
import { createTableModel } from '$lib/view/tableModel.svelte';

export const PAGE_SIZE = 10;

export function createSessionsModel() {
	let sessions = $state<TrackingSession[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let expandedSessionId = $state<string | null>(null);
	let expandedDetail = $state<SessionDetail | null>(null);
	let loadingDetail = $state(false);
	let confirmDeleteId = $state<string | null>(null);
	let deleting = $state(false);

	// Pure pager over the loaded list: no search, category, or sort, so the
	// paged rows keep the backend's ordering unchanged.
	const table = createTableModel<TrackingSession>({
		rows: () => sessions,
		pageSize: PAGE_SIZE,
	});

	async function loadSessions() {
		loading = true;
		error = null;
		try {
			sessions = await getTrackingSessions();
		} catch (e) {
			error = describeError(e, 'Failed to load sessions');
		} finally {
			loading = false;
		}
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
		toggleSession,
		handleDelete,
		collapseAll,
		expandAtIndex,
	};
}

export type SessionsModel = ReturnType<typeof createSessionsModel>;
